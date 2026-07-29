/* eslint-disable react-refresh/only-export-components */

// Shared state for the Hikmalayer Wallet browser extension.
//
// This is a CONTEXT, not a bare hook: connection state must be shared across
// every panel. (An earlier per-component version meant connecting in the
// wallet panel left the DEX panels still believing they were disconnected.)
//
// When the extension is present it is the preferred signer: the key lives in
// the extension's own context, so an XSS in this page cannot reach it, and
// every signature is approved in extension UI this page cannot draw over.
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

const ExtensionContext = createContext(null);

const getProvider = () =>
  typeof window !== "undefined" && window.hikmalayer?.isHikmalayerWallet
    ? window.hikmalayer
    : null;

export const ExtensionProvider = ({ children }) => {
  const [provider, setProvider] = useState(getProvider);
  const [account, setAccount] = useState(null);
  const [publicKey, setPublicKey] = useState(null);
  const [error, setError] = useState(null);

  // The provider is injected at document_start, but React may mount first.
  useEffect(() => {
    if (provider) return undefined;
    const onReady = () => setProvider(getProvider());
    window.addEventListener("hikmalayer#initialized", onReady);
    const timer = setTimeout(onReady, 400);
    return () => {
      window.removeEventListener("hikmalayer#initialized", onReady);
      clearTimeout(timer);
    };
  }, [provider]);

  // Restore an existing connection without prompting.
  useEffect(() => {
    if (!provider) return;
    provider
      .getAccounts()
      .then(async (accounts) => {
        if (!accounts?.length) return;
        setAccount(accounts[0]);
        setPublicKey(await provider.getPublicKey().catch(() => null));
      })
      .catch(() => {});
  }, [provider]);

  // Locking or switching accounts in the extension updates every panel.
  useEffect(() => {
    if (!provider?.on) return undefined;
    const offLock = provider.on("lock", () => setAccount(null));
    const offAccounts = provider.on("accountsChanged", (accounts) =>
      setAccount(accounts?.[0] ?? null)
    );
    return () => {
      offLock?.();
      offAccounts?.();
    };
  }, [provider]);

  const connect = useCallback(async () => {
    if (!provider) return null;
    setError(null);
    try {
      const accounts = await provider.connect();
      const next = accounts?.[0] ?? null;
      setAccount(next);
      setPublicKey(await provider.getPublicKey().catch(() => null));
      return next;
    } catch (err) {
      setError(err.message);
      return null;
    }
  }, [provider]);

  /// Ask the extension to sign. Resolves with the compact signature hex; the
  /// private key never enters this page.
  const sign = useCallback(
    async (message) => {
      if (!provider) throw new Error("Extension not available");
      const result = await provider.signMessage(message);
      if (result?.publicKey) setPublicKey(result.publicKey);
      return result.signature;
    },
    [provider]
  );

  const value = useMemo(
    () => ({
      available: !!provider,
      connected: !!account,
      account,
      publicKey,
      error,
      connect,
      sign,
    }),
    [provider, account, publicKey, error, connect, sign]
  );

  return <ExtensionContext.Provider value={value}>{children}</ExtensionContext.Provider>;
};

export const useExtension = () => {
  const context = useContext(ExtensionContext);
  if (!context) {
    throw new Error("useExtension must be used within ExtensionProvider");
  }
  return context;
};
