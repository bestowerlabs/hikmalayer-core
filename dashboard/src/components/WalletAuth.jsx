import React, { useState } from "react";
import { useWallet } from "../hooks/useWallet";

const WalletAuth = () => {
  const {
    account,
    isConnected,
    isLoading,
    error,
    connectWallet,
    disconnectWallet,
  } = useWallet();
  const [input, setInput] = useState("");

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6 max-w-md">
      <div
        className={`absolute inset-0 ${
          isConnected
            ? "bg-gradient-to-br from-green-500/10 to-emerald-500/10"
            : "bg-gradient-to-br from-blue-500/10 to-purple-500/10"
        }`}
      ></div>

      <div className="relative z-10">
        <div className="flex items-center gap-4 mb-4">
          <div
            className={`p-3 rounded-xl ${
              isConnected
                ? "bg-gradient-to-r from-green-500/20 to-emerald-500/20"
                : "bg-gradient-to-r from-blue-500/20 to-purple-500/20"
            }`}
          >
            <span className="text-2xl">{isConnected ? "🟢" : "🔴"}</span>
          </div>
          <div className="flex-1">
            <h3 className="text-lg font-bold text-white">
              {isConnected ? "Identity Connected" : "Connect Your Identity"}
            </h3>
            {!isConnected && (
              <p className="text-gray-300 text-sm">
                Enter your native hkm… address (from{" "}
                <code className="text-cyan-300">hikma-wallet keygen</code>)
              </p>
            )}
          </div>
        </div>

        {isConnected ? (
          <div className="space-y-4">
            <div className="flex items-center gap-2 p-3 bg-white/10 rounded-xl backdrop-blur-sm border border-white/20">
              <span className="text-sm font-medium text-gray-400">
                Account:
              </span>
              <code className="text-white font-mono text-sm bg-white/10 px-2 py-1 rounded border border-white/20">
                {account?.slice(0, 6)}...{account?.slice(-4)}
              </code>
              <button
                onClick={() => navigator.clipboard.writeText(account)}
                className="text-gray-400 hover:text-white text-sm transition-colors duration-300 p-1 rounded hover:bg-white/10"
                title="Copy full address"
              >
                📋
              </button>
            </div>

            <div className="p-3 bg-white/10 rounded-xl backdrop-blur-sm border border-white/20">
              <h4 className="text-sm font-medium text-green-300 mb-2">
                🔓 What you can do:
              </h4>
              <ul className="space-y-1 text-xs text-gray-300">
                <li>✅ Sign transfers & stakes offline with hikma-wallet</li>
                <li>✅ Check balances & the on-chain validator set</li>
                <li>✅ Propose blocks as a validator</li>
                <li>✅ Explore chain state & the state root</li>
              </ul>
            </div>

            <button
              onClick={disconnectWallet}
              className="w-full bg-gradient-to-r from-red-500 to-red-600 text-white px-4 py-3 rounded-xl font-medium transition-all duration-300 hover:scale-105 hover:shadow-lg hover:shadow-red-500/25"
            >
              Disconnect
            </button>
          </div>
        ) : (
          <div className="space-y-4">
            {error && (
              <div className="p-3 bg-red-500/20 border border-red-500/30 text-red-300 rounded-xl backdrop-blur-sm">
                {error}
              </div>
            )}
            <input
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder="hkm… address"
              className="w-full bg-white/10 border border-white/20 rounded-xl px-4 py-3 text-white placeholder-gray-400 font-mono text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
            <button
              onClick={() => connectWallet(input)}
              disabled={isLoading}
              className="w-full relative overflow-hidden bg-gradient-to-r from-blue-500 to-purple-600 text-white px-4 py-3 rounded-xl font-medium transition-all duration-300 hover:scale-105 hover:shadow-lg hover:shadow-blue-500/25 disabled:opacity-50 disabled:cursor-not-allowed group"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-blue-600 to-purple-700 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
              <span className="relative z-10 flex items-center justify-center gap-2">
                {isLoading ? (
                  <>
                    <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></div>
                    Connecting…
                  </>
                ) : (
                  <>
                    <span>🔑</span>
                    Use Identity
                  </>
                )}
              </span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
};

export default WalletAuth;
