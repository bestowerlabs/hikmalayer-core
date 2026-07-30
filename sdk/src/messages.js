// Canonical signing messages.
//
// These strings ARE the authorization. The node reconstructs each message
// from the request fields and checks the signature against it, so a message
// built even slightly differently — a reordered field, a decimal amount where
// base units were meant, a boolean rendered "True" instead of "true" — is not
// a bug that shows up as a wrong number. It shows up as "signature
// verification failed", with nothing to indicate which of a dozen fields was
// wrong.
//
// So every builder here takes explicit base-unit amounts and renders them the
// one way the chain expects. `test/conformance.test.mjs` signs each of these
// with the `hikma-wallet` CLI and asserts the signatures are byte-identical,
// which is the only way to be sure this file has not drifted from the Rust.

import { encodeAmount } from "./units.js";

/// Domain prefix for every native signature (mirrors `src/consensus/pos.rs`).
export const MESSAGE_PREFIX = "\x19Hikmalayer Signed Message:\n";

/// Address prefix.
export const ADDRESS_PREFIX = "hkm";

const bool = (value) => (value ? "true" : "false");

/// Move native HKM.
export const transfer = ({ from, to, amount, nonce }) =>
  `hikmalayer-transfer:${from}:${to}:${encodeAmount(amount)}:${nonce}`;

/// Bond stake to become (or top up) a validator. Uniquely among these
/// messages, the VRF public key is part of what is signed — it binds the
/// leader-election identity to the stake.
export const stake = ({ address, amount, nonce, vrfPublicKey }) =>
  `hikmalayer-stake:${address}:${encodeAmount(amount)}:${nonce}:${vrfPublicKey}`;

/// Begin unbonding stake.
export const withdraw = ({ address, amount, nonce }) =>
  `hikmalayer-withdraw:${address}:${encodeAmount(amount)}:${nonce}`;

/// Lock a grant with a cliff and linear release.
export const vest = ({ from, to, amount, cliffBlocks, durationBlocks, nonce }) =>
  `hikmalayer-vest:${from}:${to}:${encodeAmount(amount)}:${cliffBlocks}:${durationBlocks}:${nonce}`;

/// Issue a native token (HTS).
export const tokenCreate = ({ symbol, name, decimals, initialSupply, nonce }) =>
  `hikmalayer-token-create:${symbol}:${name}:${decimals}:${encodeAmount(initialSupply)}:${nonce}`;

/// Move token units.
export const tokenTransfer = ({ tokenId, to, amount, nonce }) =>
  `hikmalayer-token-transfer:${tokenId}:${to}:${encodeAmount(amount)}:${nonce}`;

/// Destroy the sender's token units, reducing supply.
export const tokenBurn = ({ tokenId, amount, nonce }) =>
  `hikmalayer-token-burn:${tokenId}:${encodeAmount(amount)}:${nonce}`;

/// Deposit into an HKM/token pool. `minShares` is the slippage bound.
export const ammAdd = ({ tokenId, amountHkm, amountToken, minShares, nonce }) =>
  `hikmalayer-amm-add:${tokenId}:${encodeAmount(amountHkm)}:${encodeAmount(amountToken)}` +
  `:${encodeAmount(minShares)}:${nonce}`;

/// Burn LP shares and withdraw the underlying assets.
export const ammRemove = ({ tokenId, shares, minHkm, minToken, nonce }) =>
  `hikmalayer-amm-remove:${tokenId}:${encodeAmount(shares)}:${encodeAmount(minHkm)}` +
  `:${encodeAmount(minToken)}:${nonce}`;

/// Trade against a pool. `hkmToToken` selects the direction.
export const ammSwap = ({ tokenId, hkmToToken, amountIn, minOut, nonce }) =>
  `hikmalayer-amm-swap:${tokenId}:${bool(hkmToToken)}:${encodeAmount(amountIn)}` +
  `:${encodeAmount(minOut)}:${nonce}`;

/// Write or revoke a credential.
export const credential = ({ id, subject, dataHash, revoke, nonce }) =>
  `hikmalayer-credential:${id}:${subject}:${dataHash}:${bool(revoke)}:${nonce}`;

export const messages = {
  transfer,
  stake,
  withdraw,
  vest,
  tokenCreate,
  tokenTransfer,
  tokenBurn,
  ammAdd,
  ammRemove,
  ammSwap,
  credential,
};
