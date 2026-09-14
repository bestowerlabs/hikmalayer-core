import React, { useCallback, useEffect, useMemo, useState } from "react";
import { getAccountNonce, getTokenBalance, listValidators, stakeTokens, withdrawStake } from "../api";
import { useWallet } from "../hooks/useWallet";
import { useActiveSigner } from "../hooks/useActiveSigner";
import { HKM_DECIMALS, formatUnits, getActiveChainId, parseUnits, scoped } from "../lib/hts";

/// Stake or withdraw HKM as a validator. Mirrors TokenManager's signing
/// flow exactly — staking is a signed transaction like any other, just
/// hitting /staking/deposit or /staking/withdraw instead of /tokens/transfer.
const StakingManager = ({ refreshTrigger, onUpdate }) => {
  const { account } = useWallet();
  const { canSign, authorize } = useActiveSigner();

  const [amount, setAmount] = useState("");
  const [nonce, setNonce] = useState(null);
  const [balance, setBalance] = useState(null);
  const [validators, setValidators] = useState([]);
  const [mode, setMode] = useState("deposit"); // "deposit" | "withdraw"
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState(null);

  const refreshAccount = useCallback(async () => {
    if (!account) {
      setBalance(null);
      setNonce(null);
      return;
    }
    const [balanceResult, nonceResult] = await Promise.allSettled([
      getTokenBalance(account),
      getAccountNonce(account),
    ]);
    setBalance(
      balanceResult.status === "fulfilled" ? (balanceResult.value.data?.balance ?? null) : null
    );
    setNonce(
      nonceResult.status === "fulfilled" ? (nonceResult.value.data?.next_nonce ?? null) : null
    );
  }, [account]);

  const refreshValidators = useCallback(async () => {
    try {
      const res = await listValidators();
      setValidators(res.data?.validators ?? res.data ?? []);
    } catch {
      setValidators([]);
    }
  }, []);

  useEffect(() => {
    refreshAccount();
    refreshValidators();
  }, [refreshAccount, refreshValidators, refreshTrigger]);

  const parsed = useMemo(() => {
    try {
      return { units: parseUnits(amount, HKM_DECIMALS), error: null };
    } catch (error) {
      return { units: 0n, error: error.message };
    }
  }, [amount]);

  const canonicalMessage = scoped(
    getActiveChainId() ?? "<network>",
    `hikmalayer-${mode === "deposit" ? "stake" : "unstake"}:${account || "<from>"}` +
      `:${parsed.units}:${nonce ?? "<nonce>"}`
  );

  const exceedsBalance = mode === "deposit" && balance !== null && parsed.units > BigInt(balance);

  const ready =
    account &&
    parsed.units > 0n &&
    !exceedsBalance &&
    nonce !== null &&
    canSign;

  const submit = async (event) => {
    event.preventDefault();
    if (!ready) return;
    setBusy(true);
    setMessage(null);
    try {
      const signedBy = await authorize(canonicalMessage);
      const submitFn = mode === "deposit" ? stakeTokens : withdrawStake;
      const res = await submitFn({
        from: account,
        amount: parsed.units.toString(),
        nonce,
        ...signedBy,
      });
      const ok = res.data?.status === "success";
      setMessage({
        type: ok ? "success" : "error",
        text:
          res.data?.message ||
          (ok
            ? `${mode === "deposit" ? "Staked" : "Withdrew"} ${formatUnits(parsed.units, HKM_DECIMALS)} HKM`
            : "Transaction failed"),
      });
      if (ok) {
        setAmount("");
        await refreshAccount();
        await refreshValidators();
        onUpdate?.();
      }
    } catch (error) {
      setMessage({
        type: "error",
        text: error.response?.data?.message || error.message || "Transaction failed",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-purple-500/10 to-blue-500/10" />
      <div className="relative z-10">
        <div className="flex items-center mb-5">
          <div className="p-2 rounded-xl bg-gradient-to-r from-purple-500/20 to-blue-500/20 mr-3">
            <span className="text-2xl">🔐</span>
          </div>
          <div>
            <h2 className="text-xl font-bold text-white">Staking</h2>
            <p className="text-sm text-gray-300">Deposit or withdraw validator stake</p>
          </div>
        </div>

        {account && (
          <div className="mb-5 p-4 rounded-xl bg-gradient-to-r from-green-500/20 to-emerald-500/20 border border-green-500/30">
            <div className="text-sm text-green-300">Your balance</div>
            <div className="text-2xl font-bold text-white">
              {balance === null ? "—" : formatUnits(balance, HKM_DECIMALS)}{" "}
              <span className="text-base font-medium text-green-200">HKM</span>
            </div>
          </div>
        )}

        <div className="flex gap-2 mb-4">
          <button
            type="button"
            onClick={() => setMode("deposit")}
            className={`flex-1 px-3 py-2 rounded-lg text-sm font-medium transition ${
              mode === "deposit" ? "bg-purple-500/40 text-white" : "bg-white/5 text-gray-400"
            }`}
          >
            Deposit
          </button>
          <button
            type="button"
            onClick={() => setMode("withdraw")}
            className={`flex-1 px-3 py-2 rounded-lg text-sm font-medium transition ${
              mode === "withdraw" ? "bg-purple-500/40 text-white" : "bg-white/5 text-gray-400"
            }`}
          >
            Withdraw
          </button>
        </div>

        <form onSubmit={submit} className="space-y-3">
          <div>
            <label className="block text-xs text-gray-400 mb-1">Amount (HKM)</label>
            <input
              type="text"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              placeholder="0.0"
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-purple-500/50"
            />
            {parsed.error && (
              <p className="text-xs text-red-400 mt-1">{parsed.error}</p>
            )}
          </div>

          <button
            type="submit"
            disabled={!ready || busy}
            className="w-full px-4 py-2 rounded-lg bg-gradient-to-r from-purple-500 to-blue-600 text-white text-sm font-medium hover:from-purple-400 hover:to-blue-500 transition disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {busy ? "Submitting…" : mode === "deposit" ? "Stake HKM" : "Withdraw Stake"}
          </button>

          {message && (
            <p className={`text-sm ${message.type === "success" ? "text-green-400" : "text-red-400"}`}>
              {message.text}
            </p>
          )}
        </form>

        {validators.length > 0 && (
          <div className="mt-6 pt-5 border-t border-white/10">
            <h3 className="text-sm text-gray-400 mb-2">Active Validators</h3>
            <div className="space-y-1 max-h-40 overflow-y-auto">
              {validators.map((v, i) => (
                <div key={v.address || i} className="text-xs text-gray-300 font-mono truncate">
                  {v.address || JSON.stringify(v)}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default StakingManager;
