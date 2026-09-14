import React, { useEffect, useState } from "react";
import { getGovernance, updateGovernance } from "../api";

/// Shows current governance parameters. Updating them is an admin action
/// (the backend gates POST /governance/config on the admin token, not a
/// wallet signature), so the update form is present but expected to fail
/// gracefully with a clear message for non-admin users rather than being
/// hidden — the same "show it, let the backend say no" pattern used
/// elsewhere for admin-only actions.
const GovernanceManager = () => {
  const [config, setConfig] = useState(null);
  const [raw, setRaw] = useState("");
  const [error, setError] = useState(null);
  const [message, setMessage] = useState(null);
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      const res = await getGovernance();
      setConfig(res.data);
      setRaw(JSON.stringify(res.data, null, 2));
    } catch (err) {
      setError(err.response?.data?.message || "Could not load governance config.");
    }
  };

  useEffect(() => {
    load();
  }, []);

  const submit = async (event) => {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    try {
      const parsed = JSON.parse(raw);
      const res = await updateGovernance(parsed);
      const ok = res.data?.status === "success";
      setMessage({
        type: ok ? "success" : "error",
        text: res.data?.message || (ok ? "Updated" : "Update failed"),
      });
      if (ok) await load();
    } catch (err) {
      setMessage({
        type: "error",
        text:
          err instanceof SyntaxError
            ? "Invalid JSON."
            : err.response?.data?.message || "Update failed — this action requires admin access.",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-indigo-500/10 to-purple-500/10" />
      <div className="relative z-10">
        <div className="flex items-center mb-5">
          <div className="p-2 rounded-xl bg-gradient-to-r from-indigo-500/20 to-purple-500/20 mr-3">
            <span className="text-2xl">⚖️</span>
          </div>
          <div>
            <h2 className="text-xl font-bold text-white">Governance</h2>
            <p className="text-sm text-gray-300">Network configuration</p>
          </div>
        </div>

        {error && <p className="text-sm text-red-400 mb-3">{error}</p>}

        {config && (
          <div className="mb-4 space-y-2 text-sm">
            {Object.entries(config).map(([key, value]) => (
              <div key={key} className="flex justify-between border-b border-white/10 pb-1">
                <span className="text-gray-400">{key}</span>
                <span className="text-white font-mono">{String(value)}</span>
              </div>
            ))}
          </div>
        )}

        <details className="mt-4">
          <summary className="text-xs text-gray-400 cursor-pointer">
            Admin: edit configuration
          </summary>
          <form onSubmit={submit} className="mt-3 space-y-2">
            <textarea
              value={raw}
              onChange={(e) => setRaw(e.target.value)}
              rows={6}
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-xs font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500/50"
            />
            <button
              type="submit"
              disabled={busy}
              className="px-4 py-2 rounded-lg bg-gradient-to-r from-indigo-500 to-purple-600 text-white text-sm font-medium hover:from-indigo-400 hover:to-purple-500 transition disabled:opacity-40"
            >
              {busy ? "Updating…" : "Update Config"}
            </button>
            {message && (
              <p className={`text-sm ${message.type === "success" ? "text-green-400" : "text-red-400"}`}>
                {message.text}
              </p>
            )}
          </form>
        </details>
      </div>
    </div>
  );
};

export default GovernanceManager;
