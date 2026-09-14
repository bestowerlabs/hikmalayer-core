import { useState, useEffect } from "react";
import { getBlockchainStats, getChainId, API_BASE } from "../api";
import { setActiveChainId } from "../lib/hts";
import StatsGrid from "../components/StatsGrid";
import WalletAuth from "../components/WalletAuth";

const DashboardPage = () => {
  const [stats, setStats] = useState({
    total_blocks: 0,
    pending_transactions: 0,
    difficulty: 0,
    is_valid: false,
    latest_hash: "",
  });
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

  useEffect(() => {
    loadStats();
    const interval = setInterval(loadStats, 30000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    getChainId().then(setActiveChainId).catch(() => setActiveChainId(null));
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-24">
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
    <>
      <div className="text-center mb-8">
        <h1 className="text-5xl font-bold bg-gradient-to-r from-blue-400 via-purple-400 to-teal-400 bg-clip-text text-transparent mb-4">
          Hikmalayer Dashboard
        </h1>
        <p className="text-gray-300 text-lg mb-6">
          Next-generation blockchain platform with advanced mining,
          certificates, and token management
        </p>
        <div className="flex justify-center mb-6">
          <WalletAuth />
        </div>
        {error && (
          <div className="mt-4 p-4 bg-red-500/20 border border-red-500/30 text-red-300 rounded-xl backdrop-blur-sm">
            {error}
          </div>
        )}
      </div>
      <StatsGrid stats={stats} />
    </>
  );
};

export default DashboardPage;
