// Helpers for the native token standard (HTS) and the AMM DEX.
//
// All on-chain amounts are integer BASE UNITS. Every conversion here uses
// BigInt — never floats — so a DEX amount can never be corrupted by IEEE-754
// rounding. HKM itself has 6 decimals; each native token declares its own.

export const HKM_DECIMALS = 6;

/// Format base units for display, e.g. (1234567n, 6) -> "1.234567".
export function formatUnits(baseUnits, decimals) {
  let value;
  try {
    value = BigInt(baseUnits ?? 0);
  } catch {
    return "0";
  }
  if (decimals === 0) return value.toString();
  const negative = value < 0n;
  if (negative) value = -value;

  const scale = 10n ** BigInt(decimals);
  const whole = value / scale;
  const fraction = (value % scale).toString().padStart(decimals, "0");
  const trimmed = fraction.replace(/0+$/, "");
  const text = trimmed.length ? `${whole}.${trimmed}` : whole.toString();
  return negative ? `-${text}` : text;
}

/// Parse a decimal string into base units, e.g. ("1.5", 6) -> 1500000n.
/// Throws on malformed input or more precision than the token supports.
export function parseUnits(input, decimals) {
  const text = String(input ?? "").trim();
  if (!text) return 0n;
  if (!/^\d*\.?\d*$/.test(text)) {
    throw new Error("Amount must be a positive decimal number");
  }
  const [whole = "", fraction = ""] = text.split(".");
  if (fraction.length > decimals) {
    throw new Error(`At most ${decimals} decimal places for this asset`);
  }
  const padded = fraction.padEnd(decimals, "0");
  return BigInt(`${whole || "0"}${padded || ""}`);
}

/// Apply a slippage tolerance (percent) to an expected output, returning the
/// minimum acceptable amount as base units. This is what protects a trader
/// from an adverse price move between signing and execution.
export function applySlippage(expectedOut, slippagePercent) {
  const expected = BigInt(expectedOut ?? 0);
  // Basis points keep the math integral: 0.5% -> 50 bps.
  const bps = BigInt(Math.round(Number(slippagePercent) * 100));
  if (bps <= 0n) return expected;
  if (bps >= 10_000n) return 0n;
  return (expected * (10_000n - bps)) / 10_000n;
}

/// Spot price of a pool expressed as token per 1 HKM (display units).
export function poolPrice(pool, tokenDecimals) {
  if (!pool || !pool.reserve_hkm || !pool.reserve_token) return null;
  const hkm = Number(formatUnits(pool.reserve_hkm, HKM_DECIMALS));
  const token = Number(formatUnits(pool.reserve_token, tokenDecimals));
  if (!hkm) return null;
  return token / hkm;
}

/// Price impact of a trade, as a percentage, comparing the effective rate
/// against the pool's spot rate.
export function priceImpact(amountIn, amountOut, reserveIn, reserveOut) {
  const aIn = Number(amountIn);
  const aOut = Number(amountOut);
  const rIn = Number(reserveIn);
  const rOut = Number(reserveOut);
  if (!aIn || !aOut || !rIn || !rOut) return null;
  const spot = rOut / rIn;
  const effective = aOut / aIn;
  return Math.max(0, (1 - effective / spot) * 100);
}

// ===== Offline signing =====
//
// Private keys NEVER enter the browser. The UI builds the exact
// `hikma-wallet` command and the canonical message the node will verify;
// the user signs offline and pastes back the public key + signature.

const q = (value) => `"${String(value)}"`;

export const signingCommands = {
  swap: ({ tokenId, hkmToToken, amountIn, minOut, nonce }) =>
    `hikma-wallet sign-amm-swap ${q(tokenId)} ${hkmToToken} ${amountIn} ${minOut} ${nonce} <PRIVATE_KEY>`,
  addLiquidity: ({ tokenId, amountHkm, amountToken, minShares, nonce }) =>
    `hikma-wallet sign-amm-add ${q(tokenId)} ${amountHkm} ${amountToken} ${minShares} ${nonce} <PRIVATE_KEY>`,
  removeLiquidity: ({ tokenId, shares, minHkm, minToken, nonce }) =>
    `hikma-wallet sign-amm-remove ${q(tokenId)} ${shares} ${minHkm} ${minToken} ${nonce} <PRIVATE_KEY>`,
  createAsset: ({ symbol, name, decimals, initialSupply, nonce }) =>
    `hikma-wallet sign-token-create ${q(symbol)} ${q(name)} ${decimals} ${initialSupply} ${nonce} <PRIVATE_KEY>`,
  transferAsset: ({ tokenId, to, amount, nonce }) =>
    `hikma-wallet sign-token-transfer ${q(tokenId)} ${q(to)} ${amount} ${nonce} <PRIVATE_KEY>`,
  burnAsset: ({ tokenId, amount, nonce }) =>
    `hikma-wallet sign-token-burn ${q(tokenId)} ${amount} ${nonce} <PRIVATE_KEY>`,
};

/// The canonical messages the node verifies — shown so a user can confirm
/// exactly what they are authorizing before signing.
export const signingMessages = {
  swap: ({ tokenId, hkmToToken, amountIn, minOut, nonce }) =>
    `hikmalayer-amm-swap:${tokenId}:${hkmToToken}:${amountIn}:${minOut}:${nonce}`,
  addLiquidity: ({ tokenId, amountHkm, amountToken, minShares, nonce }) =>
    `hikmalayer-amm-add:${tokenId}:${amountHkm}:${amountToken}:${minShares}:${nonce}`,
  removeLiquidity: ({ tokenId, shares, minHkm, minToken, nonce }) =>
    `hikmalayer-amm-remove:${tokenId}:${shares}:${minHkm}:${minToken}:${nonce}`,
  createAsset: ({ symbol, name, decimals, initialSupply, nonce }) =>
    `hikmalayer-token-create:${symbol}:${name}:${decimals}:${initialSupply}:${nonce}`,
  transferAsset: ({ tokenId, to, amount, nonce }) =>
    `hikmalayer-token-transfer:${tokenId}:${to}:${amount}:${nonce}`,
  burnAsset: ({ tokenId, amount, nonce }) =>
    `hikmalayer-token-burn:${tokenId}:${amount}:${nonce}`,
};

export const shortId = (value, lead = 10, tail = 6) => {
  const text = String(value ?? "");
  return text.length > lead + tail + 1
    ? `${text.slice(0, lead)}…${text.slice(-tail)}`
    : text;
};
