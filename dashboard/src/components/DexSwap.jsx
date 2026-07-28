import React, { useCallback, useEffect, useMemo, useState } from "react";
import { getSwapQuote, listAssets, listPools, submitSwap, getAccountNonce } from "../api";
import { useWallet } from "../hooks/useWallet";
import { useActiveSigner } from "../hooks/useActiveSigner";
import { safeText } from "../lib/sanitize";
import OfflineSigner from "./OfflineSigner";
import {
  HKM_DECIMALS,
  applySlippage,
  formatUnits,
  parseUnits,
  priceImpact,
  shortId,
  signingCommands,
  signingMessages,
} from "../lib/hts";

/// Swap HKM <-> a native token against an on-chain constant-product pool.
/// Quotes come from the node's `/dex/quote`, which runs the same arithmetic
/// consensus will apply, so the preview matches execution exactly.
const DexSwap = ({ refreshTrigger, onUpdate }) => {
  const { account } = useWallet();
  const { canSign: unlocked, sign, publicKey: signerPublicKey } = useActiveSigner();
  const [pools, setPools] = useState([]);
  const [assets, setAssets] = useState([]);
  const [tokenId, setTokenId] = useState("");
  const [hkmToToken, setHkmToToken] = useState(true);
  const [amount, setAmount] = useState("");
  const [slippage, setSlippage] = useState(0.5);
  const [quote, setQuote] = useState(null);
  const [quoteError, setQuoteError] = useState("");
  const [nonce, setNonce] = useState(null);
  const [publicKey, setPublicKey] = useState("");
  const [signature, setSignature] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState(null);

  const asset = useMemo(
    () => assets.find((a) => a.token_id === tokenId) || null,
    [assets, tokenId]
  );
  const tokenDecimals = asset?.decimals ?? 0;
  const inDecimals = hkmToToken ? HKM_DECIMALS : tokenDecimals;
  const outDecimals = hkmToToken ? tokenDecimals : HKM_DECIMALS;

  const loadMarkets = useCallback(async () => {
    try {
      const [poolsRes, assetsRes] = await Promise.all([listPools(), listAssets()]);
      setPools(poolsRes.data || []);
      setAssets(assetsRes.data || []);
      setTokenId((current) => {
        if (current) return current;
        return poolsRes.data?.[0]?.token_id || "";
      });
    } catch {
      setMessage({ type: "error", text: "Could not load pools. Is the node running?" });
    }
  }, []);

  useEffect(() => {
    loadMarkets();
  }, [loadMarkets, refreshTrigger]);

  useEffect(() => {
    if (!account) return;
    getAccountNonce(account)
      .then((res) => setNonce(res.data?.next_nonce ?? null))
      .catch(() => setNonce(null));
  }, [account, refreshTrigger, message]);

  // Amount in base units, or null when the input is not yet valid.
  const amountIn = useMemo(() => {
    try {
      const parsed = parseUnits(amount, inDecimals);
      return parsed > 0n ? parsed : null;
    } catch {
      return null;
    }
  }, [amount, inDecimals]);

  // Live quote from the chain.
  useEffect(() => {
    if (!tokenId || !amountIn) {
      setQuote(null);
      setQuoteError("");
      return;
    }
    let cancelled = false;
    const direction = hkmToToken ? "hkm_to_token" : "token_to_hkm";
    getSwapQuote(tokenId, direction, amountIn.toString())
      .then((res) => {
        if (cancelled) return;
        if (!res.data) {
          setQuote(null);
          setQuoteError("No pool liquidity for this pair yet.");
        } else {
          setQuote(res.data);
          setQuoteError("");
        }
      })
      .catch(() => {
        if (!cancelled) {
          setQuote(null);
          setQuoteError("Quote unavailable.");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [tokenId, amountIn, hkmToToken]);

  const minOut = useMemo(
    () => (quote ? applySlippage(quote.amount_out, slippage) : 0n),
    [quote, slippage]
  );

  const impact = useMemo(() => {
    if (!quote || !amountIn) return null;
    const reserveIn = hkmToToken ? quote.reserve_hkm : quote.reserve_token;
    const reserveOut = hkmToToken ? quote.reserve_token : quote.reserve_hkm;
    return priceImpact(amountIn, quote.amount_out, reserveIn, reserveOut);
  }, [quote, amountIn, hkmToToken]);

  const signParams = {
    tokenId,
    hkmToToken,
    amountIn: amountIn?.toString() ?? "0",
    minOut: minOut.toString(),
    nonce: nonce ?? 0,
  };

  // With an unlocked wallet the signature is produced at submit time; a
  // locked wallet falls back to the offline (cold-key) paste flow.
  const ready =
    account && tokenId && amountIn && quote && nonce !== null &&
    (unlocked || (publicKey && signature));

  const handleSwap = async () => {
    if (!ready) return;
    setBusy(true);
    setMessage(null);
    try {
      const signedBy = unlocked
        ? { public_key: signerPublicKey, signature: await sign(signingMessages.swap(signParams)) }
        : { public_key: publicKey.trim(), signature: signature.trim() };
      const res = await submitSwap({
        token_id: tokenId,
        trader: account,
        hkm_to_token: hkmToToken,
        amount_in: Number(amountIn),
        min_out: Number(minOut),
        nonce,
        ...signedBy,
      });
      const ok = res.data?.status === "success";
      setMessage({ type: ok ? "success" : "error", text: res.data?.message });
      if (ok) {
        setSignature("");
        setAmount("");
        onUpdate?.();
      }
    } catch (error) {
      setMessage({ type: "error", text: error.message || "Swap failed" });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-teal-500/10 to-blue-500/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
      <div className="relative z-10">
        <h3 className="text-xl font-bold text-white mb-1 flex items-center gap-2">
          <span>🔄</span> Swap
        </h3>
        <p className="text-xs text-gray-400 mb-4">
          Constant-product pools · 0.30% fee to liquidity providers
        </p>

        {pools.length === 0 ? (
          <p className="text-sm text-gray-300 py-6 text-center">
            No liquidity pools yet. Create one in the Liquidity panel.
          </p>
        ) : (
          <>
            <label className="block text-xs text-gray-400 mb-1">Market</label>
            <select
              value={tokenId}
              onChange={(e) => setTokenId(e.target.value)}
              className="w-full px-3 py-2 mb-3 rounded-lg bg-white/10 border border-white/20 text-white text-sm focus:outline-none focus:ring-2 focus:ring-teal-500/50"
            >
              {pools.map((p) => {
                const meta = assets.find((a) => a.token_id === p.token_id);
                return (
                  <option key={p.token_id} value={p.token_id} className="bg-slate-800">
                    HKM / {safeText(meta?.symbol, 12) || shortId(p.token_id)}
                  </option>
                );
              })}
            </select>

            <div className="flex items-center gap-2 mb-3">
              <div className="flex-1">
                <label className="block text-xs text-gray-400 mb-1">
                  You pay ({hkmToToken ? "HKM" : safeText(asset?.symbol, 12) || "token"})
                </label>
                <input
                  type="text"
                  value={amount}
                  onChange={(e) => setAmount(e.target.value)}
                  placeholder="0.0"
                  className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-teal-500/50"
                />
              </div>
              <button
                type="button"
                onClick={() => setHkmToToken((v) => !v)}
                title="Flip direction"
                className="mt-5 px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 border border-white/20 text-white transition"
              >
                ⇅
              </button>
              <div className="flex-1">
                <label className="block text-xs text-gray-400 mb-1">
                  You receive ({hkmToToken ? safeText(asset?.symbol, 12) || "token" : "HKM"})
                </label>
                <div className="w-full px-3 py-2 rounded-lg bg-black/30 border border-white/10 text-teal-200 text-sm">
                  {quote ? formatUnits(quote.amount_out, outDecimals) : "—"}
                </div>
              </div>
            </div>

            <div className="flex items-center gap-2 mb-2">
              <label className="text-xs text-gray-400">Slippage tolerance</label>
              {[0.1, 0.5, 1].map((v) => (
                <button
                  key={v}
                  type="button"
                  onClick={() => setSlippage(v)}
                  className={`text-xs px-2 py-0.5 rounded-md border transition ${
                    slippage === v
                      ? "bg-teal-500/30 border-teal-400/50 text-teal-100"
                      : "bg-white/5 border-white/10 text-gray-300 hover:bg-white/10"
                  }`}
                >
                  {v}%
                </button>
              ))}
            </div>

            {quoteError && <p className="text-xs text-amber-300 mb-2">{quoteError}</p>}

            {quote && (
              <div className="rounded-lg bg-black/20 border border-white/10 p-3 text-xs space-y-1 mb-1">
                <div className="flex justify-between text-gray-300">
                  <span>Minimum received</span>
                  <span className="text-white">
                    {formatUnits(minOut, outDecimals)}{" "}
                    {hkmToToken ? safeText(asset?.symbol, 12) || "token" : "HKM"}
                  </span>
                </div>
                <div className="flex justify-between text-gray-300">
                  <span>Price impact</span>
                  <span className={impact > 5 ? "text-amber-300" : "text-white"}>
                    {impact === null ? "—" : `${impact.toFixed(2)}%`}
                  </span>
                </div>
                <div className="flex justify-between text-gray-300">
                  <span>Pool reserves</span>
                  <span className="text-white">
                    {formatUnits(quote.reserve_hkm, HKM_DECIMALS)} HKM /{" "}
                    {formatUnits(quote.reserve_token, tokenDecimals)}{" "}
                    {safeText(asset?.symbol, 12)}
                  </span>
                </div>
              </div>
            )}

            {!account && (
              <p className="text-xs text-amber-300 mt-3">
                Connect a native address above to swap.
              </p>
            )}

            {account && amountIn && quote && !unlocked && (
              <OfflineSigner
                command={signingCommands.swap(signParams)}
                message={signingMessages.swap(signParams)}
                publicKey={publicKey}
                signature={signature}
                onPublicKey={setPublicKey}
                onSignature={setSignature}
                disabled={busy}
              />
            )}

            <button
              type="button"
              onClick={handleSwap}
              disabled={!ready || busy}
              className="w-full mt-3 px-4 py-2.5 rounded-xl bg-gradient-to-r from-teal-500 to-blue-500 text-white font-semibold text-sm disabled:opacity-40 disabled:cursor-not-allowed hover:from-teal-400 hover:to-blue-400 transition"
            >
              {busy ? "Submitting…" : unlocked ? "Sign & swap" : "Swap"}
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
          </>
        )}
      </div>
    </div>
  );
};

export default DexSwap;
