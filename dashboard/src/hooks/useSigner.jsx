/* eslint-disable react-refresh/only-export-components */

// Wallet session.
//
// Hardening (see also the CSP in index.html):
//  * The unlocked key is NEVER held as a plain string. It is kept encrypted
//    under a per-session, NON-EXTRACTABLE AES-GCM key and decrypted to a
//    short-lived buffer only for the instant of signing, then wiped.
//  * Signing is never silent: every request raises a confirmation showing the
//    exact canonical message, so a scripted signing spree is visible and
//    refusable rather than automatic.
//  * The session is wiped on lock, on inactivity, and on tab close.
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  AUTO_LOCK_MS,
  clearVault,
  createSessionKey,
  decryptVault,
  deriveAddress,
  derivePublicKey,
  encryptVault,
  generatePrivateKey,
  isValidPrivateKey,
  loadVault,
  protectKey,
  saveVault,
  signMessageFromBytes,
  withProtectedKey,
} from "../lib/wallet";
import { deriveHybridIdentity, signHybridFromBytes } from "../lib/hybrid";
import SignConfirm from "../components/SignConfirm";

/// Which account of the unlocked key the UI is operating as.
///
/// One private key controls two DIFFERENT accounts with separate balances:
/// the classical `hkm…` one and the quantum-ready `hkq…` one. This is a
/// local preference, not a secret, so it lives in localStorage.
const SCHEME_KEY = "hikmalayer.wallet.scheme";

const readScheme = () => (localStorage.getItem(SCHEME_KEY) === "hybrid" ? "hybrid" : "classical");

const SignerContext = createContext(null);

export const useSigner = () => {
  const context = useContext(SignerContext);
  if (!context) throw new Error("useSigner must be used within SignerProvider");
  return context;
};

export const SignerProvider = ({ children, onUnlock }) => {
  const [vault, setVault] = useState(() => loadVault());
  const [unlocked, setUnlocked] = useState(false);
  const [error, setError] = useState(null);
  const [lastActivity, setLastActivity] = useState(Date.now());
  const [pending, setPending] = useState(null); // { message, resolve, reject }

  // Session material: a non-extractable AES-GCM key plus the key ciphertext.
  // Neither refs hold anything usable on their own.
  const sessionKeyRef = useRef(null);
  const protectedKeyRef = useRef(null);
  // The hybrid identity is derived on unlock and held in memory only. It is
  // public material, but there is no reason to persist a second copy of
  // something a key already determines.
  const [hybridIdentity, setHybridIdentity] = useState(null);
  const [scheme, setSchemeState] = useState(readScheme);

  const lock = useCallback(() => {
    sessionKeyRef.current = null;
    protectedKeyRef.current = null;
    setHybridIdentity(null);
    setUnlocked(false);
    setPending((current) => {
      current?.reject(new Error("Wallet locked"));
      return null;
    });
  }, []);

  useEffect(() => {
    if (!unlocked) return undefined;
    const timer = setInterval(() => {
      if (Date.now() - lastActivity > AUTO_LOCK_MS) lock();
    }, 15_000);
    return () => clearInterval(timer);
  }, [unlocked, lastActivity, lock]);

  useEffect(() => {
    const onHide = () => lock();
    window.addEventListener("beforeunload", onHide);
    return () => window.removeEventListener("beforeunload", onHide);
  }, [lock]);

  const touch = useCallback(() => setLastActivity(Date.now()), []);

  const openWith = useCallback(
    async (privateKeyHex, nextVault) => {
      const sessionKey = await createSessionKey();
      sessionKeyRef.current = sessionKey;
      protectedKeyRef.current = await protectKey(sessionKey, privateKeyHex);
      // ~3 ms of ML-DSA keygen, once per unlock. Doing it here means the
      // hybrid address is available to the UI without ever storing it.
      setHybridIdentity(deriveHybridIdentity(privateKeyHex));
      setVault(nextVault);
      setUnlocked(true);
      touch();
      onUnlock?.(nextVault.address);
    },
    [onUnlock, touch]
  );

  const createWallet = useCallback(
    async (password) => {
      setError(null);
      try {
        const key = generatePrivateKey();
        const nextVault = await encryptVault(key, password);
        saveVault(nextVault);
        await openWith(key, nextVault);
        return { address: nextVault.address, privateKey: key };
      } catch (err) {
        setError(err.message);
        return null;
      }
    },
    [openWith]
  );

  const importWallet = useCallback(
    async (privateKeyHex, password) => {
      setError(null);
      try {
        if (!isValidPrivateKey(privateKeyHex)) {
          throw new Error("That is not a valid 32-byte private key");
        }
        const nextVault = await encryptVault(privateKeyHex, password);
        saveVault(nextVault);
        await openWith(privateKeyHex, nextVault);
        return { address: nextVault.address };
      } catch (err) {
        setError(err.message);
        return null;
      }
    },
    [openWith]
  );

  const unlock = useCallback(
    async (password) => {
      setError(null);
      try {
        const stored = loadVault();
        if (!stored) throw new Error("No wallet on this device");
        const key = await decryptVault(stored, password);
        await openWith(key, stored);
        return true;
      } catch (err) {
        setError(err.message);
        return false;
      }
    },
    [openWith]
  );

  const removeWallet = useCallback(() => {
    lock();
    clearVault();
    setVault(null);
  }, [lock]);

  const exportPrivateKey = useCallback(async (password) => {
    const stored = loadVault();
    if (!stored) throw new Error("No wallet on this device");
    return decryptVault(stored, password);
  }, []);

  /// Ask the user, then run `consumer` over the raw key bytes.
  ///
  /// The key exists as a buffer for exactly the duration of the callback and
  /// is wiped after; it is never a string, because JS strings cannot be
  /// wiped. Every signature passes through here, so there is no silent path.
  const withApproval = useCallback(
    async (message, consumer) => {
      if (!sessionKeyRef.current || !protectedKeyRef.current) {
        throw new Error("Wallet is locked");
      }
      touch();

      // Confirmation gate: resolves when the user approves the exact message.
      await new Promise((resolve, reject) => {
        setPending({ message, resolve, reject });
      });

      return withProtectedKey(sessionKeyRef.current, protectedKeyRef.current, consumer);
    },
    [touch]
  );

  /// Sign a canonical message under the classical scheme. Returns compact
  /// ECDSA hex.
  const sign = useCallback(
    (message) => withApproval(message, (keyBytes) => signMessageFromBytes(message, keyBytes)),
    [withApproval]
  );

  /// Authorize a canonical message: the request fields the node expects.
  ///
  /// In hybrid mode this carries both signatures. The node rejects a
  /// classical (`hkm…`) transaction that carries post-quantum fields and a
  /// hybrid (`hkq…`) one that omits them, so the mode and the address cannot
  /// silently disagree — one authorized transaction, one valid encoding.
  const authorize = useCallback(
    async (message) => {
      if (scheme !== "hybrid") {
        return { public_key: vault?.publicKey, signature: await sign(message) };
      }
      if (!hybridIdentity) throw new Error("Wallet is locked");
      const both = await withApproval(message, (keyBytes) =>
        signHybridFromBytes(message, keyBytes)
      );
      return {
        public_key: hybridIdentity.publicKey,
        signature: both.signature,
        pq_public_key: hybridIdentity.pqPublicKey,
        pq_signature: both.pqSignature,
      };
    },
    [scheme, hybridIdentity, sign, vault, withApproval]
  );

  /// Switch which of the key's two accounts the UI operates as.
  const setScheme = useCallback((next) => {
    const value = next === "hybrid" ? "hybrid" : "classical";
    localStorage.setItem(SCHEME_KEY, value);
    setSchemeState(value);
  }, []);

  const approve = useCallback(() => {
    setPending((current) => {
      current?.resolve();
      return null;
    });
  }, []);

  const reject = useCallback(() => {
    setPending((current) => {
      current?.reject(new Error("Signature rejected"));
      return null;
    });
  }, []);

  const value = useMemo(
    () => ({
      hasWallet: !!vault,
      unlocked,
      // The active account follows the selected scheme. Hybrid needs the
      // wallet unlocked, because the address depends on the ML-DSA key.
      address:
        scheme === "hybrid"
          ? hybridIdentity?.address ?? null
          : vault?.address ?? null,
      publicKey: vault?.publicKey ?? null,
      classicalAddress: vault?.address ?? null,
      hybridAddress: hybridIdentity?.address ?? null,
      pqPublicKey: hybridIdentity?.pqPublicKey ?? null,
      scheme,
      setScheme,
      authorize,
      error,
      createWallet,
      importWallet,
      unlock,
      lock,
      removeWallet,
      exportPrivateKey,
      sign,
      touch,
      derivePublicKey,
      deriveAddress,
    }),
    [
      vault,
      unlocked,
      error,
      createWallet,
      importWallet,
      unlock,
      lock,
      removeWallet,
      exportPrivateKey,
      sign,
      authorize,
      scheme,
      setScheme,
      hybridIdentity,
      touch,
    ]
  );

  return (
    <SignerContext.Provider value={value}>
      {children}
      <SignConfirm
        request={pending}
        address={scheme === "hybrid" ? hybridIdentity?.address : vault?.address}
        onApprove={approve}
        onReject={reject}
      />
    </SignerContext.Provider>
  );
};
