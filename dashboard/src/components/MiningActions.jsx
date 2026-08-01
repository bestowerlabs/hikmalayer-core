import React, { useState, useEffect } from "react";
import { useAuthenticatedApi } from "../hooks/useAuthenticatedApi";
import { API_BASE } from "../api";

const MiningActions = ({ refreshTrigger, onUpdate }) => {
  const [difficulty, setDifficulty] = useState(2);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [currentDifficulty, setCurrentDifficulty] = useState(2);
  const [miningStats, setMiningStats] = useState(null);

  const { authenticatedFetch, isConnected } = useAuthenticatedApi();

  useEffect(() => {
    fetchCurrentDifficulty();
  }, [refreshTrigger]);

  const fetchCurrentDifficulty = async () => {
    try {
      const response = await fetch(`${API_BASE}/mining/difficulty`);
      if (response.ok) {
        const result = await response.json();
        setCurrentDifficulty(result.current_difficulty);
        setDifficulty(result.current_difficulty);
      }
    } catch (error) {
      console.error("Failed to fetch difficulty:", error);
      setMessage(`Failed to fetch difficulty: ${error.message}`);
    }
  };

  const handleMine = async () => {
    if (!isConnected) {
      setMessage("Please connect your wallet first");
      return;
    }

    setLoading(true);
    setMessage("Mining in progress...");
    setMiningStats(null);

    try {
      const response = await authenticatedFetch("/mine", {
        method: "POST",
      });

      const result = await response.json();

      if (response.ok) {
        setMessage(`${result.message}`);
        setMiningStats({
          blockIndex: result.block_index,
          transactionsCount: result.transactions_count,
          status: result.status,
        });

        // Trigger refresh in parent components
        if (onUpdate) {
          onUpdate();
        }
      } else {
        setMessage(`Mining failed: ${result.message || "Unknown error"}`);
      }
    } catch (error) {
      setMessage(`Mining error: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  const handleSetDifficulty = async (e) => {
    e.preventDefault();

    if (!isConnected) {
      setMessage("Please connect your wallet first");
      return;
    }

    setLoading(true);

    try {
      const response = await authenticatedFetch("/mining/difficulty", {
        method: "POST",
        body: JSON.stringify({ difficulty: parseInt(difficulty) }),
      });

      const result = await response.json();

      if (response.ok) {
        setMessage(`${result.message}`);
        setCurrentDifficulty(parseInt(difficulty));
      } else {
        setMessage(
          `Failed to set difficulty: ${result.message || "Unknown error"}`
        );
      }
    } catch (error) {
      setMessage(`Error: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  const getDifficultyInfo = (diff) => {
    switch (diff) {
      case 1:
        return {
          level: "Very Easy",
          time: "~1s",
          color: "from-green-500/20 to-emerald-500/20",
          border: "border-green-500/30",
        };
      case 2:
        return {
          level: "Easy",
          time: "~2-5s",
          color: "from-blue-500/20 to-cyan-500/20",
          border: "border-blue-500/30",
        };
      case 3:
        return {
          level: "Medium",
          time: "~10-30s",
          color: "from-yellow-500/20 to-orange-500/20",
          border: "border-yellow-500/30",
        };
      case 4:
        return {
          level: "Hard",
          time: "~1-5min",
          color: "from-orange-500/20 to-red-500/20",
          border: "border-orange-500/30",
        };
      case 5:
        return {
          level: "Very Hard",
          time: "~5-15min",
          color: "from-red-500/20 to-pink-500/20",
          border: "border-red-500/30",
        };
      case 6:
        return {
          level: "Extreme",
          time: "~15min+",
          color: "from-purple-500/20 to-indigo-500/20",
          border: "border-purple-500/30",
        };
      default:
        return {
          level: "Unknown",
          time: "?",
          color: "from-gray-500/20 to-slate-500/20",
          border: "border-gray-500/30",
        };
    }
  };

  const currentDiffInfo = getDifficultyInfo(currentDifficulty);

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-orange-500/10 to-red-500/10"></div>

      <div className="relative z-10">
        <div className="flex items-center mb-6">
          <div className="p-2 rounded-xl bg-gradient-to-r from-orange-500/20 to-red-500/20 mr-3">
            <span className="text-2xl">⛏️</span>
          </div>
          <h2 className="text-xl font-bold text-white">Mining Operations</h2>
        </div>

        {!isConnected && (
          <div className="mb-6 p-4 rounded-xl bg-yellow-500/20 text-yellow-300 border border-yellow-500/30">
            <div className="flex items-center gap-2">
              <span className="text-lg">⚠️</span>
              <span className="font-medium">
                Please connect your wallet to use mining features
              </span>
            </div>
          </div>
        )}

        {/* Status Message */}
        {message && (
          <div
            className={`mb-6 p-4 rounded-xl backdrop-blur-sm border transition-all duration-500 ${
              message.includes("Successfully") || message.includes("✅")
                ? "bg-green-500/20 text-green-300 border-green-500/30"
                : message.includes("Mining in progress") ||
                  message.includes("⛏️")
                ? "bg-blue-500/20 text-blue-300 border-blue-500/30 animate-pulse"
                : "bg-red-500/20 text-red-300 border-red-500/30"
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="text-lg">
                {message.includes("Successfully") || message.includes("✅")
                  ? "✅"
                  : message.includes("Mining in progress") ||
                    message.includes("⛏️")
                  ? "⛏️"
                  : "❌"}
              </span>
              <span className="font-medium">{message}</span>
            </div>
          </div>
        )}

        {/* Mining Stats */}
        {miningStats && (
          <div className="mb-6 p-4 rounded-xl bg-gradient-to-r from-green-500/20 to-emerald-500/20 border border-green-500/30 backdrop-blur-sm">
            <h3 className="text-lg font-semibold text-green-300 mb-3 flex items-center gap-2">
              <span>📊</span>
              <span>Mining Results</span>
            </h3>
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div className="text-green-200">
                <span className="text-green-400 font-medium">Block Index:</span>{" "}
                {miningStats.blockIndex}
              </div>
              <div className="text-green-200">
                <span className="text-green-400 font-medium">
                  Transactions:
                </span>{" "}
                {miningStats.transactionsCount}
              </div>
            </div>
          </div>
        )}

        <div className="space-y-6">
          {/* Mining Section */}
          <div className="p-6 rounded-xl bg-gradient-to-r from-indigo-500/20 to-purple-500/20 border border-indigo-500/30 backdrop-blur-sm">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-white flex items-center gap-2">
                <span>⚡</span>
                <span>Mine New Block</span>
              </h3>
              <div
                className={`px-3 py-1 rounded-lg text-xs font-medium backdrop-blur-sm border ${currentDiffInfo.color} ${currentDiffInfo.border}`}
              >
                <div className="text-white">
                  Difficulty: {currentDifficulty}
                </div>
                <div className="text-gray-300">{currentDiffInfo.level}</div>
              </div>
            </div>

            <div className="mb-4 p-3 rounded-lg bg-white/10 backdrop-blur-sm border border-white/20">
              <div className="text-sm text-gray-300 mb-2">
                <span className="font-medium text-gray-200">
                  Current Mining Difficulty:
                </span>{" "}
                {currentDifficulty}/6
              </div>
              <div className="text-xs text-gray-400">
                <span className="font-medium">Estimated Time:</span>{" "}
                {currentDiffInfo.time} |
                <span className="font-medium"> Level:</span>{" "}
                {currentDiffInfo.level}
              </div>
            </div>

            <button
              onClick={handleMine}
              disabled={loading || !isConnected}
              className={`relative overflow-hidden w-full bg-gradient-to-r text-white px-6 py-3 rounded-xl font-medium transition-all duration-300 flex items-center justify-center gap-2 group ${
                loading || !isConnected
                  ? "from-gray-500 to-gray-600 cursor-not-allowed"
                  : "from-indigo-500 to-purple-600 hover:scale-105 hover:shadow-lg hover:shadow-indigo-500/25"
              }`}
            >
              <div className="absolute inset-0 bg-gradient-to-r from-indigo-600 to-purple-700 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
              <span
                className={`relative z-10 text-xl ${
                  loading ? "animate-spin" : ""
                }`}
              >
                {loading ? "⏳" : "⛏️"}
              </span>
              <span className="relative z-10">
                {loading
                  ? "Mining in Progress..."
                  : !isConnected
                  ? "Connect Wallet to Mine"
                  : "Start Mining"}
              </span>
            </button>
          </div>

          {/* Difficulty Control Section */}
          <form
            onSubmit={handleSetDifficulty}
            className="p-6 rounded-xl bg-gradient-to-r from-teal-500/20 to-green-500/20 border border-teal-500/30 backdrop-blur-sm"
          >
            <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
              <span>⚙️</span>
              <span>Mining Difficulty Settings</span>
            </h3>

            <div className="mb-4">
              <label className="block text-sm font-medium text-gray-200 mb-2">
                Select Difficulty Level (1-6)
              </label>
              <div className="flex items-center gap-4 mb-3">
                <input
                  type="range"
                  min="1"
                  max="6"
                  value={difficulty}
                  onChange={(e) => setDifficulty(parseInt(e.target.value))}
                  className="flex-1 h-2 bg-white/20 rounded-lg appearance-none cursor-pointer slider"
                  disabled={loading || !isConnected}
                />
                <div className="text-white font-bold text-lg min-w-[2rem] text-center">
                  {difficulty}
                </div>
              </div>

              {/* Difficulty Preview */}
              <div
                className={`p-3 rounded-lg backdrop-blur-sm border transition-all duration-300 ${
                  getDifficultyInfo(difficulty).color
                } ${getDifficultyInfo(difficulty).border}`}
              >
                <div className="text-sm text-white">
                  <div className="font-medium">
                    {getDifficultyInfo(difficulty).level}
                  </div>
                  <div className="text-xs text-gray-300">
                    Expected mining time: {getDifficultyInfo(difficulty).time}
                  </div>
                </div>
              </div>
            </div>

            <button
              type="submit"
              disabled={
                loading || difficulty === currentDifficulty || !isConnected
              }
              className="relative overflow-hidden bg-gradient-to-r from-teal-500 to-green-600 text-white px-6 py-2 rounded-xl font-medium transition-all duration-300 hover:scale-105 hover:shadow-lg hover:shadow-teal-500/25 disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2 group"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-teal-600 to-green-700 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
              <span className="relative z-10">{loading ? "⏳" : "⚙️"}</span>
              <span className="relative z-10">
                {loading
                  ? "Updating..."
                  : !isConnected
                  ? "Connect Wallet"
                  : difficulty === currentDifficulty
                  ? "Current Setting"
                  : "Update Difficulty"}
              </span>
            </button>

            <div className="mt-3 text-xs text-gray-400 bg-white/10 p-3 rounded-lg backdrop-blur-sm border border-white/20">
              <div className="font-medium text-gray-300 mb-1">
                💡 Difficulty Guide:
              </div>
              <div>
                Higher difficulty = More secure blocks + Longer mining time
              </div>
              <div>
                Lower difficulty = Faster mining + Less computational work
              </div>
            </div>
          </form>
        </div>
      </div>

      <style jsx>{`
        .slider::-webkit-slider-thumb {
          appearance: none;
          height: 20px;
          width: 20px;
          border-radius: 50%;
          background: linear-gradient(45deg, #06b6d4, #10b981);
          cursor: pointer;
          border: 2px solid white;
          box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
        }

        .slider::-moz-range-thumb {
          height: 20px;
          width: 20px;
          border-radius: 50%;
          background: linear-gradient(45deg, #06b6d4, #10b981);
          cursor: pointer;
          border: 2px solid white;
          box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
        }
      `}</style>
    </div>
  );
};

export default MiningActions;
