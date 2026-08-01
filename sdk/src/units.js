// Amounts.
//
// Every amount on Hikmalayer is an integer number of base units, and the SDK
// represents them as `BigInt` without exception. This is not fussiness: HKM
// has 6 decimals and a 100-billion supply, so the largest legitimate amount
// is 10^17 base units — well past `Number.MAX_SAFE_INTEGER` (2^53−1 ≈
// 9.007×10^15). A `number` cannot hold it.
//
// The failure is quiet and expensive. Signatures cover the exact decimal text
// of an amount, so a value that lost its low digits produces a transaction
// that no longer matches what was signed: the node rejects an honest transfer
// and nothing in the error explains why. So: BigInt in, decimal strings on
// the wire, and never a float in between.

/// Decimals of the native coin.
export const HKM_DECIMALS = 6;

/// Base units in one HKM.
export const UNITS_PER_HKM = 1_000_000n;

/// Coerce to BigInt, rejecting the lossy inputs rather than accepting them.
///
/// A `number` is allowed only when it is a safe integer — the one case where
/// it is provably exact. Anything larger must arrive as a string or BigInt.
export function toBaseUnits(value) {
  if (typeof value === "bigint") return value;
  if (typeof value === "string") {
    const text = value.trim();
    if (!/^-?\d+$/.test(text)) {
      throw new TypeError(
        `Expected a whole number of base units, got ${JSON.stringify(value)}. ` +
          `To convert a decimal amount like "1.5", use parseUnits().`
      );
    }
    return BigInt(text);
  }
  if (typeof value === "number") {
    if (!Number.isInteger(value)) {
      throw new TypeError(`Base units must be a whole number, got ${value}`);
    }
    if (!Number.isSafeInteger(value)) {
      throw new TypeError(
        `${value} exceeds Number.MAX_SAFE_INTEGER and has already lost precision. ` +
          `Pass a BigInt or a decimal string instead.`
      );
    }
    return BigInt(value);
  }
  throw new TypeError(`Cannot interpret ${typeof value} as an amount`);
}

/// Parse a human decimal string into base units: ("1.5", 6) -> 1500000n.
///
/// Rejects more precision than the asset supports rather than rounding it
/// away — a truncated amount is a different amount, and the user should hear
/// about it before signing.
export function parseUnits(input, decimals = HKM_DECIMALS) {
  const text = String(input ?? "").trim();
  if (!text) return 0n;
  if (!/^-?\d*\.?\d*$/.test(text) || text === "." || text === "-") {
    throw new TypeError(`"${input}" is not a decimal number`);
  }
  const negative = text.startsWith("-");
  const body = negative ? text.slice(1) : text;
  const [whole = "", fraction = ""] = body.split(".");
  if (fraction.length > decimals) {
    throw new TypeError(
      `This asset has ${decimals} decimals; "${input}" has ${fraction.length}`
    );
  }
  const units = BigInt(`${whole || "0"}${fraction.padEnd(decimals, "0")}`);
  return negative ? -units : units;
}

/// Format base units for display: (1234567n, 6) -> "1.234567".
export function formatUnits(baseUnits, decimals = HKM_DECIMALS) {
  let value = toBaseUnits(baseUnits);
  if (decimals === 0) return value.toString();
  const negative = value < 0n;
  if (negative) value = -value;
  const scale = 10n ** BigInt(decimals);
  const fraction = (value % scale).toString().padStart(decimals, "0").replace(/0+$/, "");
  const text = fraction ? `${value / scale}.${fraction}` : (value / scale).toString();
  return negative ? `-${text}` : text;
}

/// Serialize an amount for the wire. Always a decimal string: exact by
/// construction, and accepted by every amount field in the node API.
export function encodeAmount(value) {
  return toBaseUnits(value).toString();
}

/// Apply a slippage tolerance (in percent) to an expected output, returning
/// the minimum acceptable amount. This is what a signature commits to, and
/// therefore what stops a trade being sandwiched between signing and
/// execution. Computed in basis points so the arithmetic stays integral.
export function applySlippage(expectedOut, slippagePercent) {
  const expected = toBaseUnits(expectedOut);
  const bps = BigInt(Math.round(Number(slippagePercent) * 100));
  if (bps <= 0n) return expected;
  if (bps >= 10_000n) return 0n;
  return (expected * (10_000n - bps)) / 10_000n;
}

/// Integer square root, matching `isqrt_u128` in the node. Used for the LP
/// shares minted by a pool's first deposit.
export function isqrt(value) {
  const n = toBaseUnits(value);
  if (n < 0n) throw new RangeError("isqrt of a negative number");
  if (n < 2n) return n;
  let x = n;
  let y = (n + 1n) / 2n;
  while (y < x) {
    x = y;
    y = (x + n / x) / 2n;
  }
  return x;
}
