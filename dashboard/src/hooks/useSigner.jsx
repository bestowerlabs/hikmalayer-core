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
import SignConfirm from "../components/SignConfirm";

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

  const lock = useCallback(() => {
    sessionKeyRef.current = null;
    protectedKeyRef.current = null;
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

  /// Sign a canonical message. Always asks the user first, and only ever
  /// exposes the key as a buffer that is wiped immediately after use.
  const sign = useCallback(
    async (message) => {
      if (!sessionKeyRef.current || !protectedKeyRef.current) {
        throw new Error("Wallet is locked");
      }
      touch();

      // Confirmation gate: resolves when the user approves the exact message.
      await new Promise((resolve, reject) => {
        setPending({ message, resolve, reject });
      });

      return withProtectedKey(
        sessionKeyRef.current,
        protectedKeyRef.current,
        (keyBytes) => signMessageFromBytes(message, keyBytes)
      );
    },
    [touch]
  );

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
      address: vault?.address ?? null,
      publicKey: vault?.publicKey ?? null,
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
      touch,
    ]
  );

  return (
    <SignerContext.Provider value={value}>
      {children}
      <SignConfirm
        request={pending}
        address={vault?.address}
        onApprove={approve}
        onReject={reject}
      />
    </SignerContext.Provider>
  );
};
