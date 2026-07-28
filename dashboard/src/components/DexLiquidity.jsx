import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  addLiquidity,
  getAccountNonce,
  getLpPosition,
  listAssets,
  listPools,
  removeLiquidity,
} from "../api";
import { useWallet } from "../hooks/useWallet";
import { useSigner } from "../hooks/useSigner";
import { safeText } from "../lib/sanitize";
import OfflineSigner from "./OfflineSigner";
import {
  HKM_DECIMALS,
  formatUnits,
  parseUnits,
  poolPrice,
  shortId,
  signingCommands,
  signingMessages,
} from "../lib/hts";

/// Provide or withdraw liquidity in HKM<->token pools, and view LP positions.
/// The first deposit into a pool sets its price; later deposits must match
/// the pool ratio (the chain uses whichever side binds).
const DexLiquidity = ({ refreshTrigger, onUpdate }) => {
  const { account } = useWallet();
  const { unlocked, sign, publicKey: signerPublicKey } = useSigner();
  const [pools, setPools] = useState([]);
  const [assets, setAssets] = useState([]);
  const [mode, setMode] = useState("add");
  const [tokenId, setTokenId] = useState("");
  const [amountHkm, setAmountHkm] = useState("");
  const [amountToken, setAmountToken] = useState("");
  const [shares, setShares] = useState("");
  const [position, setPosition] = useState(null);
  const [nonce, setNonce] = useState(null);
  const [publicKey, setPublicKey] = useState("");
  const [signature, setSignature] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState(null);

  const asset = useMemo(
    () => assets.find((a) => a.token_id === tokenId) || null,
    [assets, tokenId]
  );
  const pool = useMemo(
    () => pools.find((p) => p.token_id === tokenId) || null,
    [pools, tokenId]
  );
  const tokenDecimals = asset?.decimals ?? 0;

  const load = useCallback(async () => {
    try {
      const [poolsRes, assetsRes] = await Promise.all([listPools(), listAssets()]);
      setPools(poolsRes.data || []);
      setAssets(assetsRes.data || []);
      setTokenId((current) => current || assetsRes.data?.[0]?.token_id || "");
    } catch {
      setMessage({ type: "error", text: "Could not load pools." });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load, refreshTrigger]);

  useEffect(() => {
    if (!account) return;
    getAccountNonce(account)
      .then((res) => setNonce(res.data?.next_nonce ?? null))
      .catch(() => setNonce(null));
  }, [account, refreshTrigger, message]);

  useEffect(() => {
    if (!account || !tokenId) {
      setPosition(null);
      return;
    }
    getLpPosition(tokenId, account)
      .then((res) => setPosition(res.data))
      .catch(() => setPosition(null));
  }, [account, tokenId, refreshTrigger, message]);

  const parsed = useMemo(() => {
    try {
      return {
        hkm: parseUnits(amountHkm, HKM_DECIMALS),
        token: parseUnits(amountToken, tokenDecimals),
        shares: shares ? BigInt(shares) : 0n,
        error: null,
      };
    } catch (error) {
      return { hkm: 0n, token: 0n, shares: 0n, error: error.message };
    }
  }, [amountHkm, amountToken, shares, tokenDecimals]);

  // Mirror the pool ratio so the user sees the matching side while typing.
  const suggestedToken = useMemo(() => {
    if (!pool || !pool.reserve_hkm || parsed.hkm <= 0n) return null;
    return (parsed.hkm * BigInt(pool.reserve_token)) / BigInt(pool.reserve_hkm);
  }, [pool, parsed.hkm]);

  const addParams = {
    tokenId,
    amountHkm: parsed.hkm.toString(),
    amountToken: parsed.token.toString(),
    minShares: "1",
    nonce: nonce ?? 0,
  };
  const removeParams = {
    tokenId,
    shares: parsed.shares.toString(),
    minHkm: "1",
    minToken: "1",
    nonce: nonce ?? 0,
  };

  // Unlocked wallet signs at submit time; otherwise the offline paste flow.
  const signedOk = unlocked || (publicKey && signature);
  const readyAdd =
    account && tokenId && parsed.hkm > 0n && parsed.token > 0n && nonce !== null && signedOk;
  const readyRemove =
    account && tokenId && parsed.shares > 0n && nonce !== null && signedOk;

  const submit = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const canonical =
        mode === "add"
          ? signingMessages.addLiquidity(addParams)
          : signingMessages.removeLiquidity(removeParams);
      const signedBy = unlocked
        ? { public_key: signerPublicKey, signature: await sign(canonical) }
        : { public_key: publicKey.trim(), signature: signature.trim() };
      const res =
        mode === "add"
          ? await addLiquidity({
              token_id: tokenId,
              provider: account,
              amount_hkm: Number(parsed.hkm),
              amount_token: Number(parsed.token),
              min_shares: 1,
              nonce,
              ...signedBy,
            })
          : await removeLiquidity({
              token_id: tokenId,
              provider: account,
              shares: Number(parsed.shares),
              min_hkm: 1,
              min_token: 1,
              nonce,
              ...signedBy,
            });
      const ok = res.data?.status === "success";
      setMessage({ type: ok ? "success" : "error", text: res.data?.message });
      if (ok) {
        setSignature("");
        setAmountHkm("");
        setAmountToken("");
        setShares("");
        onUpdate?.();
      }
    } catch (error) {
      setMessage({ type: "error", text: error.message || "Submission failed" });
    } finally {
      setBusy(false);
    }
  };

  const price = poolPrice(pool, tokenDecimals);

  return (
    <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-purple-500/10 to-pink-500/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
      <div className="relative z-10">
        <h3 className="text-xl font-bold text-white mb-1 flex items-center gap-2">
          <span>💧</span> Liquidity
        </h3>
        <p className="text-xs text-gray-400 mb-4">
          Earn the 0.30% swap fee pro rata on your share of a pool
        </p>

        <div className="flex gap-2 mb-4">
          {["add", "remove"].map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setMode(m)}
              className={`flex-1 px-3 py-1.5 rounded-lg text-sm capitalize transition border ${
                mode === m
                  ? "bg-purple-500/30 border-purple-400/50 text-white"
                  : "bg-white/5 border-white/10 text-gray-300 hover:bg-white/10"
              }`}
            >
              {m}
            </button>
          ))}
        </div>

        <label className="block text-xs text-gray-400 mb-1">Asset</label>
        <select
          value={tokenId}
          onChange={(e) => setTokenId(e.target.value)}
          className="w-full px-3 py-2 mb-3 rounded-lg bg-white/10 border border-white/20 text-white text-sm focus:outline-none focus:ring-2 focus:ring-purple-500/50"
        >
          {assets.length === 0 && <option value="">No assets issued yet</option>}
          {assets.map((a) => (
            <option key={a.token_id} value={a.token_id} className="bg-slate-800">
              HKM / {safeText(a.symbol, 12)}{" "}
              {pools.some((p) => p.token_id === a.token_id) ? "" : "(new pool)"}
            </option>
          ))}
        </select>

        {pool && (
          <div className="rounded-lg bg-black/20 border border-white/10 p-3 text-xs space-y-1 mb-3">
            <div className="flex justify-between text-gray-300">
              <span>Reserves</span>
              <span className="text-white">
                {formatUnits(pool.reserve_hkm, HKM_DECIMALS)} HKM /{" "}
                {formatUnits(pool.reserve_token, tokenDecimals)} {safeText(asset?.symbol, 12)}
              </span>
            </div>
            {price !== null && (
              <div className="flex justify-between text-gray-300">
                <span>Spot price</span>
                <span className="text-white">
                  1 HKM ≈ {price.toLocaleString(undefined, { maximumFractionDigits: 8 })}{" "}
                  {safeText(asset?.symbol, 12)}
                </span>
              </div>
            )}
            <div className="flex justify-between text-gray-300">
              <span>Your LP shares</span>
              <span className="text-white">
                {position?.shares ?? 0}
                {position?.total_shares
                  ? ` (${((Number(position.shares) / Number(position.total_shares)) * 100).toFixed(2)}%)`
                  : ""}
              </span>
            </div>
          </div>
        )}

        {mode === "add" ? (
          <>
            <label className="block text-xs text-gray-400 mb-1">HKM amount</label>
            <input
              type="text"
              value={amountHkm}
              onChange={(e) => setAmountHkm(e.target.value)}
              placeholder="0.0"
              className="w-full px-3 py-2 mb-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-purple-500/50"
            />
            <label className="block text-xs text-gray-400 mb-1">
              {safeText(asset?.symbol, 12) || "Token"} amount
              {suggestedToken !== null && (
                <button
                  type="button"
                  onClick={() =>
                    setAmountToken(formatUnits(suggestedToken, tokenDecimals))
                  }
                  className="ml-2 text-teal-300 hover:text-teal-200"
                >
                  use pool ratio ({formatUnits(suggestedToken, tokenDecimals)})
                </button>
              )}
            </label>
            <input
              type="text"
              value={amountToken}
              onChange={(e) => setAmountToken(e.target.value)}
              placeholder="0.0"
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-purple-500/50"
            />
            {!pool && (
              <p className="text-xs text-amber-300 mt-2">
                First deposit — your ratio sets the pool's starting price.
              </p>
            )}
          </>
        ) : (
          <>
            <label className="block text-xs text-gray-400 mb-1">
              LP shares to burn
              {position?.shares ? (
                <button
                  type="button"
                  onClick={() => setShares(String(position.shares))}
                  className="ml-2 text-teal-300 hover:text-teal-200"
                >
                  max ({position.shares})
                </button>
              ) : null}
            </label>
            <input
              type="text"
              value={shares}
              onChange={(e) => setShares(e.target.value.replace(/\D/g, ""))}
              placeholder="0"
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-purple-500/50"
            />
          </>
        )}

        {parsed.error && <p className="text-xs text-red-300 mt-2">{parsed.error}</p>}

        {!account && (
          <p className="text-xs text-amber-300 mt-3">
            Connect a native address above to provide liquidity.
          </p>
        )}

        {account && tokenId && !unlocked &&
          (mode === "add" ? parsed.hkm > 0n : parsed.shares > 0n) && (
          <OfflineSigner
            command={
              mode === "add"
                ? signingCommands.addLiquidity(addParams)
                : signingCommands.removeLiquidity(removeParams)
            }
            message={
              mode === "add"
                ? signingMessages.addLiquidity(addParams)
                : signingMessages.removeLiquidity(removeParams)
            }
            publicKey={publicKey}
            signature={signature}
            onPublicKey={setPublicKey}
            onSignature={setSignature}
            disabled={busy}
          />
        )}

        <button
          type="button"
          onClick={submit}
          disabled={busy || (mode === "add" ? !readyAdd : !readyRemove)}
          className="w-full mt-3 px-4 py-2.5 rounded-xl bg-gradient-to-r from-purple-500 to-pink-500 text-white font-semibold text-sm disabled:opacity-40 disabled:cursor-not-allowed hover:from-purple-400 hover:to-pink-400 transition"
        >
          {busy
            ? "Submitting…"
            : `${unlocked ? "Sign & " : ""}${mode === "add" ? "add liquidity" : "remove liquidity"}`}
        </button>

        {message && (
          <p
            className={`mt-3 text-xs break-words ${
              message.type === "success" ? "text-green-300" : "text-red-300"
            }`}
          >
            {message.text}
          </p>
        )}

        {pools.length > 0 && (
          <div className="mt-5 pt-4 border-t border-white/10">
            <h4 className="text-xs uppercase tracking-wide text-gray-400 mb-2">
              All pools
            </h4>
            <div className="space-y-1 max-h-40 overflow-y-auto">
              {pools.map((p) => {
                const meta = assets.find((a) => a.token_id === p.token_id);
                return (
                  <button
                    key={p.token_id}
                    type="button"
                    onClick={() => setTokenId(p.token_id)}
                    className="w-full text-left px-2 py-1.5 rounded-lg hover:bg-white/10 transition text-xs flex justify-between"
                  >
                    <span className="text-white">
                      HKM / {safeText(meta?.symbol, 12) || shortId(p.token_id)}
                    </span>
                    <span className="text-gray-400">
                      {formatUnits(p.reserve_hkm, HKM_DECIMALS)} HKM
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default DexLiquidity;
