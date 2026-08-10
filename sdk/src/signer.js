// Signing.
//
// Reproduces the chain's native scheme exactly (`src/consensus/pos.rs`):
//
//   address   = "hkm" + hex(SHA256(uncompressed secp256k1 pubkey)[..20])
//   digest    = SHA256(PREFIX + <UTF-8 BYTE length> + message)
//   signature = hex(compact ECDSA r||s), low-S normalized
//
// The length in the digest is the UTF-8 *byte* length (Rust's `str::len()`),
// not the JavaScript character count. For an ASCII message they agree; for
// anything else they do not, and the signature silently fails to verify. The
// conformance test covers a non-ASCII case for exactly this reason.

import { secp256k1 } from "@noble/curves/secp256k1";
import { sha256 } from "@noble/hashes/sha256";
import { bytesToHex, hexToBytes, utf8ToBytes } from "@noble/hashes/utils";
import { ADDRESS_PREFIX, MESSAGE_PREFIX } from "./messages.js";

export function normalizeHex(value) {
  return String(value ?? "").trim().replace(/^0x/i, "").toLowerCase();
}

/// Generate a private key with the platform CSPRNG.
export function generatePrivateKey() {
  return bytesToHex(secp256k1.utils.randomPrivateKey());
}

export function isValidPrivateKey(hex) {
  try {
    const bytes = hexToBytes(normalizeHex(hex));
    return bytes.length === 32 && secp256k1.utils.isValidPrivateKey(bytes);
  } catch {
    return false;
  }
}

/// Uncompressed (65-byte, 0x04-prefixed) public key — the form the chain
/// hashes for addresses and verifies signatures against.
export function derivePublicKey(privateKeyHex) {
  return bytesToHex(secp256k1.getPublicKey(hexToBytes(normalizeHex(privateKeyHex)), false));
}

export function deriveAddress(publicKeyHex) {
  const hash = sha256(hexToBytes(normalizeHex(publicKeyHex)));
  return ADDRESS_PREFIX + bytesToHex(hash.slice(0, 20));
}

/// Either account type is a valid destination.
///
/// `hkm…` is classical (ECDSA only), `hkq…` is hybrid (ECDSA *and*
/// ML-DSA-65). Anyone can pay either; the prefix only decides what the
/// *owner* must present to spend. Mirrors `hybrid::scheme_of` in the node.
export function isValidAddress(value) {
  return /^(hkm|hkq)[0-9a-f]{40}$/i.test(String(value ?? "").trim());
}

/// The digest the chain signs.
export function messageDigest(message) {
  const body = utf8ToBytes(message);
  const prefix = utf8ToBytes(`${MESSAGE_PREFIX}${body.length}`);
  const combined = new Uint8Array(prefix.length + body.length);
  combined.set(prefix, 0);
  combined.set(body, prefix.length);
  return sha256(combined);
}

export function signMessage(message, privateKeyHex) {
  return secp256k1
    .sign(messageDigest(message), hexToBytes(normalizeHex(privateKeyHex)))
    .toCompactHex();
}

export function verifyMessage(message, publicKeyHex, signatureHex) {
  try {
    return secp256k1.verify(
      normalizeHex(signatureHex),
      messageDigest(message),
      hexToBytes(normalizeHex(publicKeyHex))
    );
  } catch {
    return false;
  }
}

/// A signer backed by a private key held in this process.
///
/// Appropriate for scripts, tests, CI and backend services. It is NOT
/// appropriate for a treasury or validator key on a machine that browses the
/// web, and not for a browser tab — use the wallet extension there
/// (`ExtensionSigner`), or the offline `hikma-wallet` CLI.
export class LocalSigner {
  #privateKey;

  constructor(privateKeyHex) {
    if (!isValidPrivateKey(privateKeyHex)) {
      throw new Error("Not a valid 32-byte secp256k1 private key");
    }
    this.#privateKey = normalizeHex(privateKeyHex);
    this.publicKey = derivePublicKey(this.#privateKey);
    this.address = deriveAddress(this.publicKey);
  }

  static random() {
    return new LocalSigner(generatePrivateKey());
  }

  async sign(message) {
    return signMessage(message, this.#privateKey);
  }

  /// Deliberately explicit: reading the key back is a decision, not a getter.
  exportPrivateKey() {
    return this.#privateKey;
  }
}

/// A signer backed by the Hikmalayer Wallet browser extension.
///
/// The key never enters the page; each signature is approved by the user in
/// extension UI. Only usable in a browser with the extension installed.
export class ExtensionSigner {
  #provider;

  constructor(provider = globalThis.window?.hikmalayer) {
    if (!provider?.isHikmalayerWallet) {
      throw new Error("Hikmalayer Wallet extension not available");
    }
    this.#provider = provider;
    this.address = null;
    this.publicKey = null;
  }

  static isAvailable() {
    return !!globalThis.window?.hikmalayer?.isHikmalayerWallet;
  }

  /// Prompt the user to connect. Resolves with the active address.
  async connect() {
    const [address] = await this.#provider.connect();
    this.address = address;
    this.publicKey = await this.#provider.getPublicKey();
    return address;
  }

  async sign(message) {
    const result = await this.#provider.signMessage(message);
    this.publicKey = result.publicKey ?? this.publicKey;
    this.address = result.address ?? this.address;
    return result.signature;
  }
}
