// Hybrid (quantum-ready) accounts.
//
// Mirrors `src/consensus/pq.rs` and `src/consensus/hybrid.rs` exactly. A
// hybrid account is authorized by TWO signatures over the same message —
// secp256k1 ECDSA and ML-DSA-65 (FIPS 204) — and both must verify. An
// attacker therefore has to break both schemes to forge one transaction.
//
//   hkm…  classical  address = SHA256(secp_pub)[..20]
//   hkq…  hybrid     address = SHA256(domain ‖ secp_pub ‖ mldsa_pub)[..20]
//
// The hybrid address commits to BOTH public keys. That is what stops an
// attacker who broke secp256k1 from presenting the victim's classical key
// alongside an ML-DSA key of their own: substituting either key names a
// different account.
//
// # Determinism, and why it has to be exact
//
// Both the key and the signature are derived deterministically from the
// user's existing 32-byte secret, so this file and the Rust node produce
// byte-identical output from the same inputs. `test/conformance.test.mjs`
// asserts that against the `hikma-wallet` CLI. If it ever drifts, hybrid
// transactions signed here stop verifying on chain — there is no partial
// failure mode to debug, just "signature verification failed".

import { ml_dsa65 } from "@noble/post-quantum/ml-dsa.js";
import { sha256 } from "@noble/hashes/sha256";
import { bytesToHex, concatBytes, hexToBytes, utf8ToBytes } from "@noble/hashes/utils";
import { secp256k1 } from "@noble/curves/secp256k1";
import {
  derivePublicKey,
  isValidPrivateKey,
  normalizeHex,
  signMessage,
  verifyMessage,
} from "./signer.js";

/// Address prefix for hybrid accounts.
export const HYBRID_ADDRESS_PREFIX = "hkq";

/// ML-DSA-65 sizes, in bytes. Honest cost: ~5.3 KB of key and signature
/// material per hybrid transaction, against ~130 bytes classical.
export const PQ_PUBLIC_KEY_LEN = 1952;
export const PQ_SIGNATURE_LEN = 3309;

// Domain separators. Each keeps one set of 32 bytes from keying two
// different things — the reuse that turns one break into two.
const PQ_SEED_DOMAIN = utf8ToBytes("hikmalayer-pq-key-v1");
const PQ_SIGN_DOMAIN = utf8ToBytes("hikmalayer-pq-sign-v1");
const HYBRID_ADDRESS_DOMAIN = utf8ToBytes("hikmalayer-hybrid-address-v1");

/// FIPS 204's application context string. Names the chain, so an ML-DSA
/// signature made for Hikmalayer cannot be lifted into another protocol.
const PQ_CONTEXT = utf8ToBytes("hikmalayer");

function requirePrivateKey(privateKeyHex) {
  if (!isValidPrivateKey(privateKeyHex)) {
    throw new Error("Not a valid 32-byte secp256k1 private key");
  }
  return normalizeHex(privateKeyHex);
}

/// The ML-DSA seed (FIPS 204's ξ) for an account.
///
/// Derived from the master secret rather than stored separately: one backup
/// still covers both schemes.
export function derivePqSeed(privateKeyHex) {
  return sha256(concatBytes(PQ_SEED_DOMAIN, hexToBytes(requirePrivateKey(privateKeyHex))));
}

/// The account's ML-DSA-65 public key, as hex.
export function derivePqPublicKey(privateKeyHex) {
  return bytesToHex(ml_dsa65.keygen(derivePqSeed(privateKeyHex)).publicKey);
}

export function isValidPqPublicKey(value) {
  try {
    return hexToBytes(normalizeHex(value)).length === PQ_PUBLIC_KEY_LEN;
  } catch {
    return false;
  }
}

/// Sign under ML-DSA-65.
///
/// The signing randomness is a seed derived from (key, message) instead of
/// fresh entropy. This is FIPS 204's hedged variant with `rnd` pinned: it is
/// conforming, it makes signatures reproducible across implementations, and
/// it means a broken RNG on the user's machine cannot leak the key.
export function pqSignMessage(message, privateKeyHex) {
  const seed = derivePqSeed(privateKeyHex);
  const body = utf8ToBytes(message);
  return bytesToHex(
    ml_dsa65.sign(body, ml_dsa65.keygen(seed).secretKey, {
      context: PQ_CONTEXT,
      extraEntropy: sha256(concatBytes(PQ_SIGN_DOMAIN, seed, body)),
    })
  );
}

/// Verify an ML-DSA signature. Returns false for malformed input rather than
/// throwing, so a caller cannot mistake "could not check" for "valid".
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
    throw new Error(
      "classical public key must be the canonical uncompressed form (04 + 128 lower-case hex)"
    );
  }
  if (!isCanonicalPqPublicKey(pqPublicKey)) {
    throw new Error("post-quantum public key must be 1952 bytes of lower-case hex");
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
  return /^hkq[0-9a-f]{40}$/.test(String(value ?? "").trim());
}

/// Everything a hybrid account is, from the one secret the user already has.
export function deriveHybridIdentity(privateKeyHex) {
  const publicKey = derivePublicKey(requirePrivateKey(privateKeyHex));
  const pqPublicKey = derivePqPublicKey(privateKeyHex);
  return {
    address: deriveHybridAddress(publicKey, pqPublicKey),
    publicKey,
    pqPublicKey,
  };
}

/// Verify that `address` authorized `message` under both schemes.
///
/// All three checks matter; dropping any one collapses hybrid back to a
/// single scheme:
///   1. the two keys together derive to `address`
///   2. the ECDSA signature verifies
///   3. the ML-DSA signature verifies
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

/// A local signer for a hybrid (`hkq…`) account.
///
/// Same private key as `LocalSigner`, different account: `LocalSigner`
/// controls the `hkm…` address, this controls the `hkq…` one, and they hold
/// separate balances. Choose one deliberately — funds sent to the classical
/// address are not spendable by the hybrid account or the reverse.
///
/// `sign()` returns both signatures. The client detects that shape and puts
/// the post-quantum half on the wire alongside the classical one.
export class HybridSigner {
  #privateKey;

  constructor(privateKeyHex) {
    this.#privateKey = requirePrivateKey(privateKeyHex);
    const identity = deriveHybridIdentity(this.#privateKey);
    this.address = identity.address;
    this.publicKey = identity.publicKey;
    this.pqPublicKey = identity.pqPublicKey;
  }

  /// The scheme this signer authorizes for. The client reads it to decide
  /// which fields a request needs.
  get scheme() {
    return "hybrid";
  }

  async sign(message) {
    return {
      signature: signMessage(message, this.#privateKey),
      pqSignature: pqSignMessage(message, this.#privateKey),
    };
  }

  /// Deliberately explicit: reading the key back is a decision, not a getter.
  exportPrivateKey() {
    return this.#privateKey;
  }
}
