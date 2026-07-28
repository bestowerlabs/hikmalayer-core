/* eslint-disable react-refresh/only-export-components */

// Wallet session: holds the decrypted key in memory ONLY while unlocked, and
// exposes a `sign()` that never reveals it. Persistence is ciphertext only.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import {
  AUTO_LOCK_MS,
  clearVault,
  decryptVault,
  deriveAddress,
  derivePublicKey,
  encryptVault,
  generatePrivateKey,
  isValidPrivateKey,
  loadVault,
  normalizeHex,
  saveVault,
  signMessage,
} from "../lib/wallet";

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

  // The decrypted key lives in a ref, never in React state, so it is not
  // captured in devtools state snapshots or accidental renders.
  const keyRef = useRef(null);

  const lock = useCallback(() => {
    keyRef.current = null;
    setUnlocked(false);
  }, []);

  // Auto-lock after inactivity — limits exposure on an unattended machine.
  useEffect(() => {
    if (!unlocked) return undefined;
    const timer = setInterval(() => {
      if (Date.now() - lastActivity > AUTO_LOCK_MS) lock();
    }, 15_000);
    return () => clearInterval(timer);
  }, [unlocked, lastActivity, lock]);

  // Wipe the key if the tab goes away.
  useEffect(() => {
    const onHide = () => lock();
    window.addEventListener("beforeunload", onHide);
    return () => window.removeEventListener("beforeunload", onHide);
  }, [lock]);

  const touch = useCallback(() => setLastActivity(Date.now()), []);

  const openWith = useCallback(
    (privateKeyHex, nextVault) => {
      keyRef.current = normalizeHex(privateKeyHex);
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
        openWith(key, nextVault);
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
        openWith(privateKeyHex, nextVault);
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
        openWith(key, stored);
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

  /// Reveal the private key — requires the password again, even when
  /// unlocked, so a walk-up attacker cannot export it.
  const exportPrivateKey = useCallback(async (password) => {
    const stored = loadVault();
    if (!stored) throw new Error("No wallet on this device");
    return decryptVault(stored, password);
  }, []);

  /// Sign a canonical message. Throws when locked — signing is never silent.
  const sign = useCallback(
    (message) => {
      if (!keyRef.current) throw new Error("Wallet is locked");
      touch();
      return signMessage(message, keyRef.current);
    },
    [touch]
  );

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

  return <SignerContext.Provider value={value}>{children}</SignerContext.Provider>;
};
