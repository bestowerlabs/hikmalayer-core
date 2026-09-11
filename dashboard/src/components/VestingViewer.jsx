import React, { useState } from "react";
import { getVesting } from "../api";
import { useWallet } from "../hooks/useWallet";
import { HKM_DECIMALS, formatUnits } from "../lib/hts";

/// Look up a vesting schedule by address. Read-only — creating a vesting
/// schedule is an admin action (POST /tokens/vest), not something an
/// ordinary user does from this panel.
const VestingViewer = () => {
  const { account } = useWallet();
  const [address, setAddress] = useState(account || "");
  const [schedule, setSchedule] = useState(null);
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);

  const lookup = async (event) => {
    event.preventDefault();
    const target = address.trim();
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      const res = await getVesting(target);
      setSchedule(res.data);
    } catch (err) {
      setSchedule(null);
      setError(err.response?.data?.message || "No vesting schedule found for that address.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-teal-500/10 to-cyan-500/10" />
      <div className="relative z-10">
        <div className="flex items-center mb-5">
          <div className="p-2 rounded-xl bg-gradient-to-r from-teal-500/20 to-cyan-500/20 mr-3">
            <span className="text-2xl">⏳</span>
          </div>
          <div>
            <h2 className="text-xl font-bold text-white">Vesting</h2>
            <p className="text-sm text-gray-300">Check a vesting schedule</p>
          </div>
        </div>

        <form onSubmit={lookup} className="flex gap-2 mb-4">
          <input
            type="text"
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="hkm…"
            className="flex-1 px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-teal-500/50"
          />
          <button
            type="submit"
            disabled={busy}
            className="px-4 py-2 rounded-lg bg-gradient-to-r from-teal-500 to-cyan-600 text-white text-sm font-medium hover:from-teal-400 hover:to-cyan-500 transition disabled:opacity-40"
          >
            {busy ? "Checking…" : "Check"}
          </button>
        </form>

        {error && <p className="text-sm text-red-400">{error}</p>}

        {schedule && (
          <div className="space-y-2 text-sm">
            {Object.entries(schedule).map(([key, value]) => (
              <div key={key} className="flex justify-between border-b border-white/10 pb-1">
                <span className="text-gray-400">{key}</span>
                <span className="text-white font-mono">
                  {typeof value === "number" && key.toLowerCase().includes("amount")
                    ? formatUnits(value, HKM_DECIMALS)
                    : String(value)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default VestingViewer;
