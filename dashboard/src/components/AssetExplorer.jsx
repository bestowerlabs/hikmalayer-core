import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  createAsset,
  getAccountNonce,
  getAssetBalance,
  listAssets,
  transferAsset,
} from "../api";
import { useWallet } from "../hooks/useWallet";
import OfflineSigner from "./OfflineSigner";
import { formatUnits, parseUnits, shortId, signingCommands, signingMessages } from "../lib/hts";

/// Browse the native token registry, issue a new token, and send tokens.
/// Supply is fixed at issuance (reducible only by burning), so what the
/// registry reports is the real, consensus-enforced supply.
const AssetExplorer = ({ refreshTrigger, onUpdate }) => {
  const { account } = useWallet();
  const [assets, setAssets] = useState([]);
  const [mode, setMode] = useState("browse");
  const [nonce, setNonce] = useState(null);
  const [publicKey, setPublicKey] = useState("");
  const [signature, setSignature] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState(null);
  const [balances, setBalances] = useState({});

  // Issue form
  const [symbol, setSymbol] = useState("");
  const [name, setName] = useState("");
  const [decimals, setDecimals] = useState(6);
  const [supply, setSupply] = useState("");

  // Send form
  const [sendToken, setSendToken] = useState("");
  const [sendTo, setSendTo] = useState("");
  const [sendAmount, setSendAmount] = useState("");

  const load = useCallback(async () => {
    try {
      const res = await listAssets();
      setAssets(res.data || []);
      setSendToken((current) => current || res.data?.[0]?.token_id || "");
    } catch {
      setMessage({ type: "error", text: "Could not load the asset registry." });
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

  // Holdings for the connected address across every listed asset.
  useEffect(() => {
    if (!account || assets.length === 0) return;
    let cancelled = false;
    Promise.all(
      assets.map((a) =>
        getAssetBalance(a.token_id, account)
          .then((res) => [a.token_id, res.data?.balance ?? 0])
          .catch(() => [a.token_id, 0])
      )
    ).then((entries) => {
      if (!cancelled) setBalances(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
  }, [account, assets, refreshTrigger, message]);

  const sendAsset = useMemo(
    () => assets.find((a) => a.token_id === sendToken) || null,
    [assets, sendToken]
  );

  const supplyUnits = useMemo(() => {
    try {
      return parseUnits(supply, Number(decimals));
    } catch {
      return 0n;
    }
  }, [supply, decimals]);

  const sendUnits = useMemo(() => {
    try {
      return parseUnits(sendAmount, sendAsset?.decimals ?? 0);
    } catch {
      return 0n;
    }
  }, [sendAmount, sendAsset]);

  const createParams = {
    symbol,
    name,
    decimals: Number(decimals),
    initialSupply: supplyUnits.toString(),
    nonce: nonce ?? 0,
  };
  const transferParams = {
    tokenId: sendToken,
    to: sendTo,
    amount: sendUnits.toString(),
    nonce: nonce ?? 0,
  };

  const submitCreate = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const res = await createAsset({
        creator: account,
        symbol,
        name,
        decimals: Number(decimals),
        initial_supply: Number(supplyUnits),
        nonce,
        public_key: publicKey.trim(),
        signature: signature.trim(),
      });
      const ok = res.data?.status === "success";
      setMessage({ type: ok ? "success" : "error", text: res.data?.message });
      if (ok) {
        setSignature("");
        setSymbol("");
        setName("");
        setSupply("");
        onUpdate?.();
      }
    } catch (error) {
      setMessage({ type: "error", text: error.message || "Issuance failed" });
    } finally {
      setBusy(false);
    }
  };

  const submitTransfer = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const res = await transferAsset({
        token_id: sendToken,
        from: account,
        to: sendTo.trim(),
        amount: Number(sendUnits),
        nonce,
        public_key: publicKey.trim(),
        signature: signature.trim(),
      });
      const ok = res.data?.status === "success";
      setMessage({ type: ok ? "success" : "error", text: res.data?.message });
      if (ok) {
        setSignature("");
        setSendAmount("");
        onUpdate?.();
      }
    } catch (error) {
      setMessage({ type: "error", text: error.message || "Transfer failed" });
    } finally {
      setBusy(false);
    }
  };

  const readyCreate =
    account && symbol.trim() && supplyUnits > 0n && nonce !== null && publicKey && signature;
  const readySend =
    account && sendToken && sendTo.trim() && sendUnits > 0n && nonce !== null &&
    publicKey && signature;

  return (
    <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-amber-500/10 to-orange-500/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
      <div className="relative z-10">
        <h3 className="text-xl font-bold text-white mb-1 flex items-center gap-2">
          <span>🪙</span> Assets
        </h3>
        <p className="text-xs text-gray-400 mb-4">
          Native tokens — fixed supply at issuance, reducible only by burning
        </p>

        <div className="flex gap-2 mb-4">
          {["browse", "issue", "send"].map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setMode(m)}
              className={`flex-1 px-3 py-1.5 rounded-lg text-sm capitalize transition border ${
                mode === m
                  ? "bg-amber-500/30 border-amber-400/50 text-white"
                  : "bg-white/5 border-white/10 text-gray-300 hover:bg-white/10"
              }`}
            >
              {m}
            </button>
          ))}
        </div>

        {mode === "browse" && (
          <div className="space-y-2 max-h-72 overflow-y-auto">
            {assets.length === 0 && (
              <p className="text-sm text-gray-300 py-4 text-center">
                No tokens issued yet.
              </p>
            )}
            {assets.map((a) => (
              <div
                key={a.token_id}
                className="rounded-lg bg-black/20 border border-white/10 p-3"
              >
                <div className="flex justify-between items-baseline">
                  <span className="text-white font-semibold text-sm">
                    {a.symbol}
                    <span className="text-gray-400 font-normal ml-2">{a.name}</span>
                  </span>
                  <span className="text-xs text-gray-400">{a.decimals} dp</span>
                </div>
                <div className="text-[11px] text-gray-400 mt-1 break-all">
                  {a.token_id}
                </div>
                <div className="flex justify-between text-xs mt-2">
                  <span className="text-gray-300">
                    Supply{" "}
                    <span className="text-white">
                      {formatUnits(a.total_supply, a.decimals)}
                    </span>
                  </span>
                  {account && (
                    <span className="text-gray-300">
                      You hold{" "}
                      <span className="text-teal-200">
                        {formatUnits(balances[a.token_id] ?? 0, a.decimals)}
                      </span>
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}

        {mode === "issue" && (
          <>
            <div className="grid grid-cols-2 gap-2 mb-2">
              <input
                type="text"
                value={symbol}
                onChange={(e) => setSymbol(e.target.value.toUpperCase().slice(0, 12))}
                placeholder="Symbol (e.g. GOLD)"
                className="px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
              />
              <input
                type="number"
                min="0"
                max="18"
                value={decimals}
                onChange={(e) => setDecimals(e.target.value)}
                placeholder="Decimals"
                className="px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
              />
            </div>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value.slice(0, 64))}
              placeholder="Token name"
              className="w-full px-3 py-2 mb-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
            />
            <input
              type="text"
              value={supply}
              onChange={(e) => setSupply(e.target.value)}
              placeholder="Initial supply (minted to you)"
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
            />

            {account && symbol && supplyUnits > 0n && (
              <OfflineSigner
                command={signingCommands.createAsset(createParams)}
                message={signingMessages.createAsset(createParams)}
                publicKey={publicKey}
                signature={signature}
                onPublicKey={setPublicKey}
                onSignature={setSignature}
                disabled={busy}
              />
            )}

            <button
              type="button"
              onClick={submitCreate}
              disabled={!readyCreate || busy}
              className="w-full mt-3 px-4 py-2.5 rounded-xl bg-gradient-to-r from-amber-500 to-orange-500 text-white font-semibold text-sm disabled:opacity-40 disabled:cursor-not-allowed hover:from-amber-400 hover:to-orange-400 transition"
            >
              {busy ? "Submitting…" : "Issue token"}
            </button>
          </>
        )}

        {mode === "send" && (
          <>
            <select
              value={sendToken}
              onChange={(e) => setSendToken(e.target.value)}
              className="w-full px-3 py-2 mb-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm focus:outline-none focus:ring-2 focus:ring-amber-500/50"
            >
              {assets.length === 0 && <option value="">No assets issued yet</option>}
              {assets.map((a) => (
                <option key={a.token_id} value={a.token_id} className="bg-slate-800">
                  {a.symbol} — {shortId(a.token_id)}
                </option>
              ))}
            </select>
            <input
              type="text"
              value={sendTo}
              onChange={(e) => setSendTo(e.target.value)}
              placeholder="Recipient hkm… address"
              className="w-full px-3 py-2 mb-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
            />
            <input
              type="text"
              value={sendAmount}
              onChange={(e) => setSendAmount(e.target.value)}
              placeholder={`Amount${sendAsset ? ` (${sendAsset.symbol})` : ""}`}
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
            />

            {account && sendToken && sendUnits > 0n && sendTo && (
              <OfflineSigner
                command={signingCommands.transferAsset(transferParams)}
                message={signingMessages.transferAsset(transferParams)}
                publicKey={publicKey}
                signature={signature}
                onPublicKey={setPublicKey}
                onSignature={setSignature}
                disabled={busy}
              />
            )}

            <button
              type="button"
              onClick={submitTransfer}
              disabled={!readySend || busy}
              className="w-full mt-3 px-4 py-2.5 rounded-xl bg-gradient-to-r from-amber-500 to-orange-500 text-white font-semibold text-sm disabled:opacity-40 disabled:cursor-not-allowed hover:from-amber-400 hover:to-orange-400 transition"
            >
              {busy ? "Submitting…" : "Send tokens"}
            </button>
          </>
        )}

        {!account && mode !== "browse" && (
          <p className="text-xs text-amber-300 mt-3">
            Connect a native address above to continue.
          </p>
        )}

        {message && (
          <p
            className={`mt-3 text-xs break-words ${
              message.type === "success" ? "text-green-300" : "text-red-300"
            }`}
          >
            {message.text}
          </p>
        )}
      </div>
    </div>
  );
};

export default AssetExplorer;
