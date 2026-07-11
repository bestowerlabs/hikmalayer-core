/* eslint-disable react-refresh/only-export-components */

// src/hooks/useWallet.jsx
//
// Native Hikmalayer identity connector. No external wallet dependency: the
// user supplies their native `hkm…` address (from `hikma-wallet keygen`).
// Signing happens offline with the CLI and keys never enter the browser.
import { useState, useEffect, createContext, useContext } from "react";

// Wallet Context
const WalletContext = createContext();

export const useWallet = () => {
  const context = useContext(WalletContext);
  if (!context) {
    throw new Error("useWallet must be used within WalletProvider");
  }
  return context;
};

const isNativeAddress = (value) =>
  typeof value === "string" && /^hkm[0-9a-fA-F]{40}$/.test(value.trim());

export const WalletProvider = ({ children }) => {
  const [account, setAccount] = useState(null);
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState(null);

  // "Connect" a native identity by address. This is a local UI session only
  // — it grants no authority. On-chain actions are authorized by offline
  // signatures, and admin actions by the node's admin token.
  const connectWallet = async (address) => {
    setError(null);
    const candidate = (address || "").trim();
    if (!isNativeAddress(candidate)) {
      setError(
        "Enter a native Hikmalayer address (hkm… from `hikma-wallet keygen`)."
      );
      return;
    }
    setIsLoading(true);
    try {
      setAccount(candidate);
      setIsConnected(true);
      localStorage.setItem("walletAccount", candidate);
    } finally {
      setIsLoading(false);
    }
  };

  const disconnectWallet = () => {
    setAccount(null);
    setIsConnected(false);
    localStorage.removeItem("walletAccount");
  };

  // Restore a previously connected address.
  useEffect(() => {
    const saved = localStorage.getItem("walletAccount");
    if (isNativeAddress(saved)) {
      setAccount(saved);
      setIsConnected(true);
    }
  }, []);

  // The dashboard does not hold keys, so it cannot sign. Actions are signed
  // offline with `hikma-wallet`.
  const getAuthHeaders = () => ({});

  const value = {
    account,
    isConnected,
    isLoading,
    error,
    connectWallet,
    disconnectWallet,
    getAuthHeaders,
  };

  return (
    <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
  );
};

// Native identity connector component
export const WalletConnector = () => {
  const {
    account,
    isConnected,
    isLoading,
    error,
    connectWallet,
    disconnectWallet,
  } = useWallet();
  const [input, setInput] = useState("");

  if (isConnected) {
    return (
      <div className="wallet-connector connected">
        <div className="account-info">
          <span className="account-label">Identity:</span>
          <span className="account-address">
            {account?.slice(0, 8)}…{account?.slice(-4)}
          </span>
        </div>
        <button onClick={disconnectWallet} className="disconnect-button">
          Disconnect
        </button>
      </div>
    );
  }

  return (
    <div className="wallet-connector">
      {error && <div className="error-message">{error}</div>}
      <input
        type="text"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        placeholder="hkm… address"
        className="connect-input"
      />
      <button
        onClick={() => connectWallet(input)}
        disabled={isLoading}
        className="connect-button"
      >
        {isLoading ? "Connecting…" : "Use Identity"}
      </button>
    </div>
  );
};
