// Hikmalayer browser wallet — key handling, native signing, encrypted vault.
//
// SECURITY MODEL
// --------------
// * The private key is generated in the browser and NEVER leaves it. It is
//   never sent to the node, never put in a URL, never logged.
// * At rest it is stored only as AES-256-GCM ciphertext, under a key derived
//   from the user's password with PBKDF2-SHA256 (high iteration count).
//   Without the password the stored blob is useless.
// * While unlocked the key lives in memory only, and is wiped on lock,
//   on an inactivity timeout, and when the tab is closed.
// * Signing reproduces the chain's native scheme exactly (see below), so the
//   node verifies browser signatures identically to `hikma-wallet` ones.
//
// Everything here mirrors src/consensus/pos.rs:
//   address  = "hkm" + hex(SHA256(uncompressed_pubkey)[..20])
//   digest   = SHA256("\x19Hikmalayer Signed Message:\n" + len(msg) + msg)
//   signature= hex(compact ECDSA r||s), low-S normalized
import { secp256k1 } from "@noble/curves/secp256k1";
import { sha256 } from "@noble/hashes/sha256";
import { bytesToHex, hexToBytes, utf8ToBytes } from "@noble/hashes/utils";

export const ADDRESS_PREFIX = "hkm";
export const MESSAGE_PREFIX = "\x19Hikmalayer Signed Message:\n";

const VAULT_KEY = "hikmalayer.wallet.v1";
const PBKDF2_ITERATIONS = 310_000; // OWASP guidance for PBKDF2-SHA256
export const AUTO_LOCK_MS = 15 * 60 * 1000;

// ===== Keys and addresses =====

/// Generate a new private key (32 bytes) using the platform CSPRNG.
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

export function normalizeHex(value) {
  return String(value ?? "").trim().replace(/^0x/i, "").toLowerCase();
}

/// Uncompressed (65-byte, 0x04-prefixed) public key hex — the form the chain
/// hashes for addresses and uses for signature verification.
export function derivePublicKey(privateKeyHex) {
  const bytes = hexToBytes(normalizeHex(privateKeyHex));
  return bytesToHex(secp256k1.getPublicKey(bytes, false));
}

/// hkm + first 20 bytes of SHA-256 over the uncompressed public key.
export function deriveAddress(publicKeyHex) {
  const pub = hexToBytes(normalizeHex(publicKeyHex));
  const hash = sha256(pub);
  return ADDRESS_PREFIX + bytesToHex(hash.slice(0, 20));
}

/// Either account type is a payable destination.
///
/// `hkm…` is classical (ECDSA), `hkq…` is hybrid (ECDSA *and* ML-DSA-65 —
/// see lib/hybrid.js). The prefix decides what the OWNER must present to
/// spend, not who may pay in. Mirrors `hybrid::scheme_of` in the node.
export function isValidAddress(value) {
  return /^(hkm|hkq)[0-9a-f]{40}$/i.test(String(value ?? "").trim());
}

// ===== Native signing =====

/// The digest the chain signs: domain prefix + byte length + message.
/// Length is the UTF-8 BYTE length (Rust `str::len()`), not the char count.
export function messageDigest(message) {
  const body = utf8ToBytes(message);
  const prefixed = utf8ToBytes(`${MESSAGE_PREFIX}${body.length}`);
  const combined = new Uint8Array(prefixed.length + body.length);
  combined.set(prefixed, 0);
  combined.set(body, prefixed.length);
  return sha256(combined);
}

/// Sign a message under the native domain. Returns compact r||s hex, low-S
/// normalized — exactly what `pos::verify_message` accepts.
export function signMessage(message, privateKeyHex) {
  const digest = messageDigest(message);
  const signature = secp256k1.sign(digest, hexToBytes(normalizeHex(privateKeyHex)));
  return signature.toCompactHex();
}

/// Sign directly from raw key BYTES. Preferred at runtime: no private-key
/// string is ever materialized (JS strings are immutable and cannot be
/// wiped), so the only copy is a buffer the caller zeroizes immediately.
export function signMessageFromBytes(message, privateKeyBytes) {
  return secp256k1.sign(messageDigest(message), privateKeyBytes).toCompactHex();
}

/// Overwrite a byte buffer in place. Not a guarantee against a determined
/// attacker (the JS engine may still hold copies), but it removes the
/// obvious long-lived one.
export function wipe(bytes) {
  if (bytes && typeof bytes.fill === "function") bytes.fill(0);
}

// ===== Session protection for the unlocked key =====
//
// While unlocked, the key is NOT kept as a plain string. It is held encrypted
// under a per-session AES-GCM key that is generated non-extractable: even
// script running in this page cannot read that key out of WebCrypto. The
// private key is decrypted to a short-lived buffer only for the instant of
// signing, and wiped straight after.
//
// Honest limit: script executing in the page can still *call* the signer.
// This narrows the window and defeats passive scraping of memory/state — it
// is not a substitute for the CSP, and not equal to a hardware signer.

export async function createSessionKey() {
  return subtle().generateKey({ name: "AES-GCM", length: 256 }, false, [
    "encrypt",
    "decrypt",
  ]);
}

export async function protectKey(sessionKey, privateKeyHex) {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const raw = hexToBytes(normalizeHex(privateKeyHex));
  try {
    const ciphertext = await subtle().encrypt({ name: "AES-GCM", iv }, sessionKey, raw);
    return { iv, ciphertext };
  } finally {
    wipe(raw);
  }
}

/// Decrypt the protected key, hand the raw bytes to `consumer`, then wipe
/// them. The bytes never escape this function.
export async function withProtectedKey(sessionKey, protectedKey, consumer) {
  const plaintext = new Uint8Array(
    await subtle().decrypt(
      { name: "AES-GCM", iv: protectedKey.iv },
      sessionKey,
      protectedKey.ciphertext
    )
  );
  try {
    return consumer(plaintext);
  } finally {
    wipe(plaintext);
  }
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

// ===== Encrypted vault (AES-256-GCM + PBKDF2) =====

const subtle = () => {
  const c = globalThis.crypto?.subtle;
  if (!c) throw new Error("WebCrypto unavailable — use a secure (https/localhost) context");
  return c;
};

async function deriveVaultKey(password, salt, iterations) {
  const material = await subtle().importKey(
    "raw",
    utf8ToBytes(password),
    "PBKDF2",
    false,
    ["deriveKey"]
  );
  return subtle().deriveKey(
    { name: "PBKDF2", salt, iterations, hash: "SHA-256" },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"]
  );
}

const b64 = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes)));
const unb64 = (text) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0));

/// Encrypt a private key under a password. The result is safe to persist:
/// it reveals nothing without the password.
export async function encryptVault(privateKeyHex, password) {
  if (!isValidPrivateKey(privateKeyHex)) throw new Error("Invalid private key");
  if (!password || password.length < 8) {
    throw new Error("Password must be at least 8 characters");
  }
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveVaultKey(password, salt, PBKDF2_ITERATIONS);
  const ciphertext = await subtle().encrypt(
    { name: "AES-GCM", iv },
    key,
    utf8ToBytes(normalizeHex(privateKeyHex))
  );
  const publicKey = derivePublicKey(privateKeyHex);
  return {
    version: 1,
    kdf: "PBKDF2-SHA256",
    iterations: PBKDF2_ITERATIONS,
    salt: b64(salt),
    iv: b64(iv),
    ciphertext: b64(ciphertext),
    // Public metadata only — lets the UI show the address while locked.
    address: deriveAddress(publicKey),
    publicKey,
  };
}

/// Decrypt a vault. A wrong password fails AES-GCM authentication and throws.
export async function decryptVault(vault, password) {
  const key = await deriveVaultKey(
    password,
    unb64(vault.salt),
    vault.iterations || PBKDF2_ITERATIONS
  );
  let plaintext;
  try {
    plaintext = await subtle().decrypt(
      { name: "AES-GCM", iv: unb64(vault.iv) },
      key,
      unb64(vault.ciphertext)
    );
  } catch {
    throw new Error("Incorrect password");
  }
  const privateKeyHex = new TextDecoder().decode(plaintext);
  if (!isValidPrivateKey(privateKeyHex)) throw new Error("Vault is corrupt");
  // Defence in depth: the decrypted key must match the vault's stated
  // address, so a tampered `address` field can never mislead the UI.
  if (deriveAddress(derivePublicKey(privateKeyHex)) !== vault.address) {
    throw new Error("Vault integrity check failed");
  }
  return privateKeyHex;
}

// ===== Multi-account keyring (vault v2) =====
//
// A v1 vault holds exactly one key. A keyring holds several, encrypted
// together under one password, with public-only metadata alongside so the UI
// can list accounts while locked. One password and one KDF run for the whole
// set: adding an account must never mean a second thing to remember.
//
// `decryptKeyring` also reads a v1 vault, so an existing single-key install
// keeps working and is upgraded in place the next time it is written.

export const MAX_ACCOUNTS = 20;

const defaultLabel = (index) => `Account ${index + 1}`;

/// Public metadata for one key. Contains nothing secret.
function accountMetadata(privateKeyHex, label, index) {
  const publicKey = derivePublicKey(privateKeyHex);
  return {
    address: deriveAddress(publicKey),
    publicKey,
    label: String(label || defaultLabel(index)).slice(0, 40),
  };
}

/// Encrypt a set of private keys under one password.
export async function encryptKeyring(privateKeys, password, options = {}) {
  const keys = privateKeys.map(normalizeHex);
  if (keys.length === 0) throw new Error("A keyring needs at least one key");
  if (keys.length > MAX_ACCOUNTS) {
    throw new Error(`At most ${MAX_ACCOUNTS} accounts per wallet`);
  }
  if (keys.some((key) => !isValidPrivateKey(key))) {
    throw new Error("Invalid private key");
  }
  if (new Set(keys).size !== keys.length) {
    throw new Error("That key is already in this wallet");
  }
  if (!password || password.length < 8) {
    throw new Error("Password must be at least 8 characters");
  }

  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveVaultKey(password, salt, PBKDF2_ITERATIONS);
  const ciphertext = await subtle().encrypt(
    { name: "AES-GCM", iv },
    key,
    utf8ToBytes(JSON.stringify(keys))
  );

  const labels = options.labels || [];
  const accounts = keys.map((k, i) => accountMetadata(k, labels[i], i));
  const activeIndex = Math.min(Math.max(options.activeIndex ?? 0, 0), keys.length - 1);

  return {
    version: 2,
    kdf: "PBKDF2-SHA256",
    iterations: PBKDF2_ITERATIONS,
    salt: b64(salt),
    iv: b64(iv),
    ciphertext: b64(ciphertext),
    accounts,
    activeIndex,
  };
}

/// Decrypt a keyring to its private keys, in account order.
/// Accepts a v1 (single-key) vault and returns a one-element array.
export async function decryptKeyring(vault, password) {
  if (!vault) throw new Error("No wallet on this device");
  if (vault.version !== 2) {
    // Legacy single-key vault: reuse its verified path verbatim.
    return [await decryptVault(vault, password)];
  }

  const key = await deriveVaultKey(
    password,
    unb64(vault.salt),
    vault.iterations || PBKDF2_ITERATIONS
  );
  let plaintext;
  try {
    plaintext = await subtle().decrypt(
      { name: "AES-GCM", iv: unb64(vault.iv) },
      key,
      unb64(vault.ciphertext)
    );
  } catch {
    throw new Error("Incorrect password");
  }

  let keys;
  try {
    keys = JSON.parse(new TextDecoder().decode(plaintext));
  } catch {
    throw new Error("Vault is corrupt");
  }
  if (!Array.isArray(keys) || keys.length === 0) throw new Error("Vault is corrupt");
  if (keys.some((k) => !isValidPrivateKey(k))) throw new Error("Vault is corrupt");

  // Same defence in depth as v1: the stored public metadata must match what
  // the decrypted keys actually derive to, so tampering with `accounts`
  // cannot make the UI show — or sign for — the wrong address.
  const accounts = vault.accounts || [];
  if (accounts.length !== keys.length) throw new Error("Vault integrity check failed");
  keys.forEach((k, i) => {
    if (deriveAddress(derivePublicKey(k)) !== accounts[i].address) {
      throw new Error("Vault integrity check failed");
    }
  });

  return keys.map(normalizeHex);
}

/// Index of the active account, clamped into range.
export function activeAccountIndex(vault) {
  const count = vault?.accounts?.length ?? 0;
  if (count === 0) return 0;
  return Math.min(Math.max(vault.activeIndex ?? 0, 0), count - 1);
}

/// Read a vault's accounts as public metadata, upgrading a v1 vault's shape
/// on the fly so callers only ever deal with one format.
export function vaultAccounts(vault) {
  if (!vault) return [];
  if (vault.version === 2) return vault.accounts ?? [];
  return [{ address: vault.address, publicKey: vault.publicKey, label: defaultLabel(0) }];
}

// ===== Persistence (ciphertext only) =====

export function loadVault() {
  try {
    const raw = localStorage.getItem(VAULT_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function saveVault(vault) {
  localStorage.setItem(VAULT_KEY, JSON.stringify(vault));
}

export function clearVault() {
  localStorage.removeItem(VAULT_KEY);
}
