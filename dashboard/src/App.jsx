import React, { useState, useEffect } from "react";
import { WalletProvider, useWallet } from "./hooks/useWallet";
import { SignerProvider } from "./hooks/useSigner";
import { ExtensionProvider } from "./hooks/useExtension";
import { getBlockchainStats, API_BASE } from "./api";
import StatsGrid from "./components/StatsGrid";
import CertificateManager from "./components/CertificateManager";
import TokenManager from "./components/TokenManager";
import MiningActions from "./components/MiningActions";
import BlockchainViewer from "./components/BlockchainViewer";
import WalletAuth from "./components/WalletAuth";
import ProtectedAction from "./components/ProtectedAction";
import DexSwap from "./components/DexSwap";
import DexLiquidity from "./components/DexLiquidity";
import AssetExplorer from "./components/AssetExplorer";
import WalletPanel from "./components/WalletPanel";

const AppContent = () => {
  const { connectWallet } = useWallet();
  const [stats, setStats] = useState({
    total_blocks: 0,
    pending_transactions: 0,
    difficulty: 0,
    is_valid: false,
    latest_hash: "",
  });
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const loadStats = async () => {
    try {
      setError(null);
      const response = await getBlockchainStats();
      setStats(response.data);
    } catch (error) {
      console.error("Error loading stats:", error);
      setError(
        `Failed to load blockchain statistics. Check that a node is reachable at ${API_BASE}.`
      );
    } finally {
      setLoading(false);
    }
  };

  const handleUpdate = () => {
    loadStats();
    setRefreshTrigger((prev) => prev + 1);
  };

  useEffect(() => {
    loadStats();
    // Auto-refresh stats every 30 seconds
    const interval = setInterval(loadStats, 30000);
    return () => clearInterval(interval);
  }, []);

  if (loading) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900 flex items-center justify-center">
        <div className="text-center">
          <div className="relative">
            <div className="w-32 h-32 border-4 border-blue-500/20 border-t-blue-500 rounded-full animate-spin mx-auto mb-4"></div>
            <div
              className="absolute inset-0 w-32 h-32 border-4 border-purple-500/20 border-t-purple-500 rounded-full animate-spin mx-auto"
              style={{ animationDirection: "reverse" }}
            ></div>
          </div>
          <h2 className="text-2xl font-semibold text-white mb-2">
            Loading Hikmalayer...
          </h2>
          <p className="text-gray-400">Initializing blockchain interface</p>
        </div>
      </div>
    );
  }

  return (
    // One wallet identity for the whole page: the extension (preferred) and
    // the in-page wallet are shared context, so every panel below signs as
    // the same account rather than each keeping its own idea of who is
    // connected.
    <ExtensionProvider>
    <SignerProvider onUnlock={connectWallet}>
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900 relative overflow-hidden">
      {/* Animated Background Elements */}
      <div className="absolute inset-0 overflow-hidden">
        <div className="absolute -top-40 -right-40 w-80 h-80 bg-blue-500/10 rounded-full blur-3xl animate-pulse"></div>
        <div
          className="absolute -bottom-40 -left-40 w-80 h-80 bg-purple-500/10 rounded-full blur-3xl animate-pulse"
          style={{ animationDelay: "1000ms" }}
        ></div>
        <div
          className="absolute top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 w-96 h-96 bg-teal-500/5 rounded-full blur-3xl animate-pulse"
          style={{ animationDelay: "2000ms" }}
        ></div>
      </div>

      <div className="relative z-10 p-4 md:p-8">
        <div className="max-w-7xl mx-auto space-y-8">
          {/* Header with Wallet Integration */}
          <div className="text-center mb-8">
            <h1 className="text-5xl font-bold bg-gradient-to-r from-blue-400 via-purple-400 to-teal-400 bg-clip-text text-transparent mb-4">
              Hikmalayer Dashboard
            </h1>
            <p className="text-gray-300 text-lg mb-6">
              Next-generation blockchain platform with advanced mining,
              certificates, and token management
            </p>

            {/* Wallet Authentication Component */}
            <div className="flex justify-center mb-6">
              <WalletAuth />
            </div>

            {error && (
              <div className="mt-4 p-4 bg-red-500/20 border border-red-500/30 text-red-300 rounded-xl backdrop-blur-sm">
                {error}
              </div>
            )}
          </div>

          {/* Stats Grid */}
          <StatsGrid stats={stats} />

          {/* Main Content Grid with Protected Actions */}
          <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
            <ProtectedAction
              fallback={
                <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6 hover:bg-white/15 transition-all duration-500">
                  <div className="absolute inset-0 bg-gradient-to-br from-yellow-500/10 to-orange-500/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
                  <div className="relative z-10 text-center">
                    <div className="text-4xl mb-4">🔒</div>
                    <h3 className="text-xl font-bold text-white mb-2">
                      Authentication Required
                    </h3>
                    <p className="text-gray-300">
                      Connect your wallet to manage certificates
                    </p>
                  </div>
                </div>
              }
            >
              <CertificateManager onUpdate={handleUpdate} refreshTrigger={refreshTrigger} />
            </ProtectedAction>

            <ProtectedAction
              fallback={
                <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6 hover:bg-white/15 transition-all duration-500">
                  <div className="absolute inset-0 bg-gradient-to-br from-yellow-500/10 to-orange-500/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
                  <div className="relative z-10 text-center">
                    <div className="text-4xl mb-4">🔒</div>
                    <h3 className="text-xl font-bold text-white mb-2">
                      Authentication Required
                    </h3>
                    <p className="text-gray-300">
                      Connect your wallet to transfer tokens
                    </p>
                  </div>
                </div>
              }
            >
              <TokenManager onUpdate={handleUpdate} refreshTrigger={refreshTrigger} />
            </ProtectedAction>

            <ProtectedAction
              fallback={
                <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6 hover:bg-white/15 transition-all duration-500">
                  <div className="absolute inset-0 bg-gradient-to-br from-yellow-500/10 to-orange-500/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
                  <div className="relative z-10 text-center">
                    <div className="text-4xl mb-4">🔒</div>
                    <h3 className="text-xl font-bold text-white mb-2">
                      Authentication Required
                    </h3>
                    <p className="text-gray-300">
                      Connect your wallet to mine blocks
                    </p>
                  </div>
                </div>
              }
            >
              <MiningActions onUpdate={handleUpdate} refreshTrigger={refreshTrigger} />
            </ProtectedAction>
          </div>

          {/* Ecosystem: native assets + the on-chain AMM DEX. Read-only
              browsing needs no wallet; actions are authorized by offline
              signatures, so no auth gate is required here. */}
          {/* Unlocking the wallet also sets the active address for the
              address-based panels, so there is one identity on screen. */}
            <div className="mb-4">
              <h2 className="text-2xl font-bold text-white">
                Ecosystem{" "}
                <span className="bg-gradient-to-r from-teal-400 to-purple-400 bg-clip-text text-transparent">
                  DEX
                </span>
              </h2>
              <p className="text-gray-400 text-sm">
                Native assets and constant-product HKM pools. Trades HKM and
                Hikmalayer-issued tokens only — this is not a bridge to other
                chains.
              </p>
            </div>
            <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
              <WalletPanel refreshTrigger={refreshTrigger} />
              <DexSwap refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />
              <DexLiquidity refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />
              <AssetExplorer refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />
            </div>

          {/* Blockchain Viewer - No auth required (read-only) */}
          <BlockchainViewer refreshTrigger={refreshTrigger} />
        </div>
      </div>
    </div>
    </SignerProvider>
    </ExtensionProvider>
  );
};

const App = () => {
  return (
    <WalletProvider>
      <AppContent />
    </WalletProvider>
  );
};

export default App;
