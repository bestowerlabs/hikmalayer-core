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
///
/// Reserves can exceed `Number.MAX_SAFE_INTEGER`, so the ratio is taken in
/// BigInt and only the final, already-small quotient becomes a float.
export function poolPrice(pool, tokenDecimals) {
  if (!pool) return null;
  let rh;
  let rt;
  try {
    rh = BigInt(pool.reserve_hkm ?? 0);
    rt = BigInt(pool.reserve_token ?? 0);
  } catch {
    return null;
  }
  if (rh <= 0n || rt <= 0n) return null;

  // token/HKM in display units, scaled by PRICE_SCALE to keep 8 decimals.
  const numerator = rt * 10n ** BigInt(HKM_DECIMALS) * PRICE_SCALE;
  const denominator = rh * 10n ** BigInt(tokenDecimals);
  return Number(numerator / denominator) / Number(PRICE_SCALE);
}

const PRICE_SCALE = 100_000_000n; // 8 decimal places of display precision

/// Price impact of a trade, as a percentage, comparing the effective rate
/// against the pool's spot rate. Exact in BigInt up to the final display
/// conversion — reserves routinely exceed the float-safe range.
export function priceImpact(amountIn, amountOut, reserveIn, reserveOut) {
  let aIn;
  let aOut;
  let rIn;
  let rOut;
  try {
    aIn = BigInt(amountIn ?? 0);
    aOut = BigInt(amountOut ?? 0);
    rIn = BigInt(reserveIn ?? 0);
    rOut = BigInt(reserveOut ?? 0);
  } catch {
    return null;
  }
  if (aIn <= 0n || aOut <= 0n || rIn <= 0n || rOut <= 0n) return null;

  // impact = (1 - effective/spot) * 100, where effective/spot simplifies to
  // (amountOut * reserveIn) / (amountIn * reserveOut).
  const ratio = (aOut * rIn * PRICE_SCALE) / (aIn * rOut);
  if (ratio >= PRICE_SCALE) return 0;
  return (Number(PRICE_SCALE - ratio) / Number(PRICE_SCALE)) * 100;
}

// ===== Liquidity-provision quoting =====
//
// These mirror `ChainState::apply` in `src/blockchain/state.rs` exactly, so
// the shares and withdrawal amounts previewed here are the ones consensus
// will produce. That matters for more than display: the slippage bounds a
// user signs are derived from these numbers, and a bound that does not match
// the chain's arithmetic either fails honest transactions or protects nobody.

/// Consensus constant: shares permanently locked on a pool's first deposit,
/// so `total_shares` can never return to zero and re-open the ratio.
export const MINIMUM_LIQUIDITY = 1_000n;

/// Integer square root (Newton's method), matching `isqrt_u128` in the node.
export function isqrt(value) {
  const n = BigInt(value);
  if (n < 2n) return n;
  let x = n;
  let y = (x + 1n) / 2n;
  while (y < x) {
    x = y;
    y = (x + n / x) / 2n;
  }
  return x;
}

/// Predict an AddLiquidity outcome.
///
/// Returns the amounts the chain will actually take (the pool ratio binds on
/// whichever side is scarcer, so one of them is usually less than offered)
/// and the LP shares minted. `null` when the inputs cannot produce a deposit.
export function quoteAddLiquidity(pool, amountHkm, amountToken) {
  const offeredHkm = BigInt(amountHkm ?? 0);
  const offeredToken = BigInt(amountToken ?? 0);
  if (offeredHkm <= 0n || offeredToken <= 0n) return null;

  const rh = BigInt(pool?.reserve_hkm ?? 0);
  const rt = BigInt(pool?.reserve_token ?? 0);
  const totalShares = BigInt(pool?.total_shares ?? 0);

  // First deposit: shares = sqrt(hkm · token) - MINIMUM_LIQUIDITY.
  if (!pool || rh === 0n || rt === 0n || totalShares === 0n) {
    const shares0 = isqrt(offeredHkm * offeredToken);
    if (shares0 <= MINIMUM_LIQUIDITY) return null;
    return {
      first: true,
      useHkm: offeredHkm,
      useToken: offeredToken,
      minted: shares0 - MINIMUM_LIQUIDITY,
    };
  }

  const tokenOptimal = (offeredHkm * rt) / rh;
  let useHkm;
  let useToken;
  if (tokenOptimal <= offeredToken) {
    useHkm = offeredHkm;
    useToken = tokenOptimal;
  } else {
    useHkm = (offeredToken * rh) / rt;
    useToken = offeredToken;
  }
  if (useHkm === 0n || useToken === 0n) return null;

  const sharesFromHkm = (useHkm * totalShares) / rh;
  const sharesFromToken = (useToken * totalShares) / rt;
  const minted = sharesFromHkm < sharesFromToken ? sharesFromHkm : sharesFromToken;
  if (minted === 0n) return null;

  return { first: false, useHkm, useToken, minted };
}

/// Predict a RemoveLiquidity outcome: the pro-rata share of both reserves.
export function quoteRemoveLiquidity(pool, shares) {
  const burn = BigInt(shares ?? 0);
  if (burn <= 0n) return null;
  const rh = BigInt(pool?.reserve_hkm ?? 0);
  const rt = BigInt(pool?.reserve_token ?? 0);
  const totalShares = BigInt(pool?.total_shares ?? 0);
  if (totalShares <= 0n || rh <= 0n || rt <= 0n) return null;

  const amountHkm = (burn * rh) / totalShares;
  const amountToken = (burn * rt) / totalShares;
  if (amountHkm === 0n || amountToken === 0n) return null;
  return { amountHkm, amountToken };
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
