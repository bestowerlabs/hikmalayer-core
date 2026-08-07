// Hybrid (quantum-ready) accounts for the browser wallet and the extension.
//
// Mirrors `src/consensus/pq.rs` and `src/consensus/hybrid.rs` byte for byte;
// `sdk/test/hybrid.test.mjs` proves the parity against the real signer.
//
// WHY
// ---
// secp256k1 falls to Shor's algorithm, and this chain publishes public keys:
// every transaction carries one. A hybrid account is authorized by TWO
// signatures over the same message — ECDSA *and* ML-DSA-65 (FIPS 204) — so
// forging one requires breaking both. The account is safe while either holds.
//
//   hkm…  classical  address = SHA256(secp_pub)[..20]
//   hkq…  hybrid     address = SHA256(domain ‖ secp_pub ‖ mldsa_pub)[..20]
//
// The hybrid address commits to BOTH keys. Without that, an attacker who
// broke secp256k1 could pair the victim's classical key with an ML-DSA key of
// their own and the "hybrid" account would fall to a single break.
//
// COST
// ----
// ~5.3 KB of key and signature material per transaction against ~130 bytes,
// and ~11 ms to sign in this browser rather than well under one. That is the
// real price of post-quantum signatures today, and it is why hybrid is opt-in
// per account rather than forced on everyone.
import { ml_dsa65 } from "@noble/post-quantum/ml-dsa.js";
import { sha256 } from "@noble/hashes/sha256";
import { bytesToHex, concatBytes, hexToBytes, utf8ToBytes } from "@noble/hashes/utils";
import { secp256k1 } from "@noble/curves/secp256k1";

import { derivePublicKey, normalizeHex, signMessageFromBytes, verifyMessage } from "./wallet.js";

export const HYBRID_ADDRESS_PREFIX = "hkq";
export const PQ_PUBLIC_KEY_LEN = 1952;
export const PQ_SIGNATURE_LEN = 3309;

// Domain separators: the same 32 bytes must never key two different schemes.
const PQ_SEED_DOMAIN = utf8ToBytes("hikmalayer-pq-key-v1");
const PQ_SIGN_DOMAIN = utf8ToBytes("hikmalayer-pq-sign-v1");
const HYBRID_ADDRESS_DOMAIN = utf8ToBytes("hikmalayer-hybrid-address-v1");

/// FIPS 204 application context. Names the chain, so a signature made here
/// cannot be replayed into another protocol that also uses ML-DSA.
const PQ_CONTEXT = utf8ToBytes("hikmalayer");

/// The ML-DSA seed (FIPS 204's ξ) from raw private-key BYTES.
///
/// Takes bytes, not a hex string, for the same reason the classical signer
/// does: JS strings are immutable and cannot be wiped, so the runtime path
/// never materializes one.
export function pqSeedFromBytes(privateKeyBytes) {
  return sha256(concatBytes(PQ_SEED_DOMAIN, privateKeyBytes));
}

export function derivePqSeed(privateKeyHex) {
  const bytes = hexToBytes(normalizeHex(privateKeyHex));
  try {
    return pqSeedFromBytes(bytes);
  } finally {
    bytes.fill(0);
  }
}

export function pqPublicKeyFromBytes(privateKeyBytes) {
  return bytesToHex(ml_dsa65.keygen(pqSeedFromBytes(privateKeyBytes)).publicKey);
}

export function derivePqPublicKey(privateKeyHex) {
  const bytes = hexToBytes(normalizeHex(privateKeyHex));
  try {
    return pqPublicKeyFromBytes(bytes);
  } finally {
    bytes.fill(0);
  }
}

export function isValidPqPublicKey(value) {
  try {
    return hexToBytes(normalizeHex(value)).length === PQ_PUBLIC_KEY_LEN;
  } catch {
    return false;
  }
}

/// Sign under ML-DSA-65 from raw key bytes.
///
/// The signing randomness is derived from (key, message) rather than drawn
/// fresh — FIPS 204's hedged variant with `rnd` pinned. Conforming, and it
/// means a browser with a compromised RNG cannot leak the key through a
/// signature.
export function pqSignMessageFromBytes(message, privateKeyBytes) {
  const seed = pqSeedFromBytes(privateKeyBytes);
  const body = utf8ToBytes(message);
  return bytesToHex(
    ml_dsa65.sign(body, ml_dsa65.keygen(seed).secretKey, {
      context: PQ_CONTEXT,
      extraEntropy: sha256(concatBytes(PQ_SIGN_DOMAIN, seed, body)),
    })
  );
}

export function pqVerifyMessage(message, pqPublicKeyHex, pqSignatureHex) {
  try {
    const publicKey = hexToBytes(normalizeHex(pqPublicKeyHex));
    const signature = hexToBytes(normalizeHex(pqSignatureHex));
    if (publicKey.length !== PQ_PUBLIC_KEY_LEN) return false;
    if (signature.length !== PQ_SIGNATURE_LEN) return false;
    return ml_dsa65.verify(signature, utf8ToBytes(message), publicKey, { context: PQ_CONTEXT });
  } catch {
    return false;
  }
}

/// A public key's ONE canonical spelling: uncompressed, 65 bytes, `04`-
/// prefixed, lower-case hex.
///
/// secp256k1 accepts a 33-byte compressed encoding of the same key, and hex
/// accepts upper case. Both hash to the same address and verify the same
/// signatures, so allowing them would give one authorized transaction more
/// than one valid on-wire form — and therefore more than one transaction id.
/// The node rejects them; mirrors `pos::canonical_public_key`.
export function isCanonicalPublicKey(value) {
  const hex = String(value ?? "").trim();
  if (!/^04[0-9a-f]{128}$/.test(hex)) return false;
  try {
    return bytesToHex(secp256k1.ProjectivePoint.fromHex(hex).toRawBytes(false)) === hex;
  } catch {
    return false;
  }
}

/// The same rule for an ML-DSA key: lower-case hex, exact length.
export function isCanonicalPqPublicKey(value) {
  return /^[0-9a-f]+$/.test(String(value ?? "")) && isValidPqPublicKey(value);
}

/// Derive a hybrid address from both public keys.
export function deriveHybridAddress(publicKeyHex, pqPublicKeyHex) {
  const publicKey = String(publicKeyHex ?? "").trim();
  const pqPublicKey = String(pqPublicKeyHex ?? "").trim();
  // Canonical only — and a malformed point is rejected on the way, so it can
  // never reach the hash and become an address nobody could ever sign for.
  if (!isCanonicalPublicKey(publicKey)) {
    throw new Error("Not a canonical uncompressed secp256k1 public key");
  }
  if (!isCanonicalPqPublicKey(pqPublicKey)) {
    throw new Error("Not a canonical ML-DSA-65 public key");
  }
  const hash = sha256(
    concatBytes(
      HYBRID_ADDRESS_DOMAIN,
      secp256k1.ProjectivePoint.fromHex(publicKey).toRawBytes(false),
      hexToBytes(pqPublicKey)
    )
  );
  return HYBRID_ADDRESS_PREFIX + bytesToHex(hash.slice(0, 20));
}

export function isHybridAddress(value) {
  return /^hkq[0-9a-f]{40}$/i.test(String(value ?? "").trim());
}

/// Both identities of one key, from raw bytes.
///
/// The classical and hybrid addresses are DIFFERENT ACCOUNTS with separate
/// balances. One key, two accounts — not one account with two names.
export function hybridIdentityFromBytes(privateKeyBytes) {
  const publicKey = bytesToHex(secp256k1.getPublicKey(privateKeyBytes, false));
  const pqPublicKey = pqPublicKeyFromBytes(privateKeyBytes);
  return {
    address: deriveHybridAddress(publicKey, pqPublicKey),
    publicKey,
    pqPublicKey,
  };
}

export function deriveHybridIdentity(privateKeyHex) {
  const publicKey = derivePublicKey(privateKeyHex);
  const pqPublicKey = derivePqPublicKey(privateKeyHex);
  return { address: deriveHybridAddress(publicKey, pqPublicKey), publicKey, pqPublicKey };
}

/// Sign a message under both schemes from raw key bytes.
export function signHybridFromBytes(message, privateKeyBytes) {
  return {
    signature: signMessageFromBytes(message, privateKeyBytes),
    pqSignature: pqSignMessageFromBytes(message, privateKeyBytes),
  };
}

/// Verify that `address` authorized `message`. All three checks matter:
/// the keys must derive to the address, and both signatures must verify.
export function verifyHybrid(
  address,
  message,
  publicKeyHex,
  pqPublicKeyHex,
  signatureHex,
  pqSignatureHex
) {
  let derived;
  try {
    derived = deriveHybridAddress(publicKeyHex, pqPublicKeyHex);
  } catch {
    return false;
  }
  if (derived !== String(address ?? "").trim()) return false;
  if (!verifyMessage(message, publicKeyHex, signatureHex)) return false;
  return pqVerifyMessage(message, pqPublicKeyHex, pqSignatureHex);
}
