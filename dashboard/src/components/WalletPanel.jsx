import React, { useEffect, useState } from "react";
import { useSigner } from "../hooks/useSigner";
import { getTokenBalance } from "../api";
import { formatUnits, HKM_DECIMALS } from "../lib/hts";

/// The Hikmalayer wallet: create or import a key, unlock it with a password,
/// and sign transactions in-browser. The key is stored only as AES-GCM
/// ciphertext and is never sent anywhere.
const WalletPanel = ({ refreshTrigger }) => {
  const {
    hasWallet,
    unlocked,
    address,
    error,
    createWallet,
    importWallet,
    unlock,
    lock,
    removeWallet,
    exportPrivateKey,
  } = useSigner();

  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [importKey, setImportKey] = useState("");
  const [view, setView] = useState("unlock");
  const [notice, setNotice] = useState(null);
  const [freshKey, setFreshKey] = useState(null);
  const [revealed, setRevealed] = useState(null);
  const [balance, setBalance] = useState(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  const secureContext =
    typeof window !== "undefined" && (window.isSecureContext ?? true);

  useEffect(() => {
    setView(hasWallet ? "unlock" : "create");
  }, [hasWallet]);

  useEffect(() => {
    if (!address) return;
    getTokenBalance(address)
      .then((res) => setBalance(res.data?.balance ?? 0))
      .catch(() => setBalance(null));
  }, [address, refreshTrigger, unlocked]);

  const reset = () => {
    setPassword("");
    setConfirm("");
    setImportKey("");
    setNotice(null);
  };

  const handleCreate = async () => {
    if (password !== confirm) return setNotice({ type: "error", text: "Passwords do not match" });
    setBusy(true);
    const result = await createWallet(password);
    setBusy(false);
    if (result) {
      setFreshKey(result.privateKey);
      reset();
    }
  };

  const handleImport = async () => {
    if (password !== confirm) return setNotice({ type: "error", text: "Passwords do not match" });
    setBusy(true);
    const result = await importWallet(importKey.trim(), password);
    setBusy(false);
    if (result) {
      reset();
      setNotice({ type: "success", text: "Wallet imported and encrypted on this device." });
    }
  };

  const handleUnlock = async () => {
    setBusy(true);
    const ok = await unlock(password);
    setBusy(false);
    if (ok) reset();
  };

  const handleExport = async () => {
    setBusy(true);
    try {
      setRevealed(await exportPrivateKey(password));
      setPassword("");
    } catch (err) {
      setNotice({ type: "error", text: err.message });
    } finally {
      setBusy(false);
    }
  };

  const copyAddress = async () => {
    await navigator.clipboard.writeText(address);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const field =
    "w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-emerald-500/50";
  const primary =
    "w-full mt-3 px-4 py-2.5 rounded-xl bg-gradient-to-r from-emerald-500 to-teal-500 text-white font-semibold text-sm disabled:opacity-40 disabled:cursor-not-allowed hover:from-emerald-400 hover:to-teal-400 transition";

  return (
    <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-emerald-500/10 to-teal-500/10 opacity-0 group-hover:opacity-100 transition-opacity duration-500" />
      <div className="relative z-10">
        <div className="flex items-center justify-between mb-1">
          <h3 className="text-xl font-bold text-white flex items-center gap-2">
            <span>👛</span> Wallet
          </h3>
          {hasWallet && (
            <span
              className={`text-xs px-2 py-0.5 rounded-md border ${
                unlocked
                  ? "bg-emerald-500/20 border-emerald-400/40 text-emerald-200"
                  : "bg-white/5 border-white/15 text-gray-300"
              }`}
            >
              {unlocked ? "Unlocked" : "Locked"}
            </span>
          )}
        </div>
        <p className="text-xs text-gray-400 mb-4">
          Keys are encrypted on this device and never sent to the node.
        </p>

        {!secureContext && (
          <p className="text-xs text-red-300 mb-3">
            ⚠️ Insecure page — open over HTTPS or localhost to use the wallet.
          </p>
        )}

        {address && (
          <div className="rounded-lg bg-black/20 border border-white/10 p-3 mb-3">
            <div className="flex items-center justify-between mb-1">
              <span className="text-xs uppercase tracking-wide text-gray-400">
                Address
              </span>
              <button
                type="button"
                onClick={copyAddress}
                className="text-xs px-2 py-0.5 rounded-md bg-white/10 hover:bg-white/20 text-gray-200 transition"
              >
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
            <code className="block break-all text-[11px] text-emerald-200">{address}</code>
            <div className="text-xs text-gray-300 mt-2">
              Balance:{" "}
              <span className="text-white">
                {balance === null ? "—" : `${formatUnits(balance, HKM_DECIMALS)} HKM`}
              </span>
            </div>
          </div>
        )}

        {/* Newly created key — the one moment it is shown, for backup. */}
        {freshKey && (
          <div className="rounded-lg bg-amber-500/10 border border-amber-400/30 p-3 mb-3">
            <p className="text-xs text-amber-200 font-semibold mb-1">
              Back this up now — it cannot be recovered.
            </p>
            <code className="block break-all text-[11px] text-amber-100 mb-2">
              {freshKey}
            </code>
            <button
              type="button"
              onClick={() => setFreshKey(null)}
              className="text-xs px-2 py-1 rounded-md bg-amber-500/30 hover:bg-amber-500/40 text-white transition"
            >
              I have saved it
            </button>
          </div>
        )}

        {revealed && (
          <div className="rounded-lg bg-red-500/10 border border-red-400/30 p-3 mb-3">
            <p className="text-xs text-red-200 font-semibold mb-1">Private key</p>
            <code className="block break-all text-[11px] text-red-100 mb-2">{revealed}</code>
            <button
              type="button"
              onClick={() => setRevealed(null)}
              className="text-xs px-2 py-1 rounded-md bg-red-500/30 hover:bg-red-500/40 text-white transition"
            >
              Hide
            </button>
          </div>
        )}

        {!unlocked && (
          <>
            <div className="flex gap-2 mb-3">
              {(hasWallet ? ["unlock", "import"] : ["create", "import"]).map((v) => (
                <button
                  key={v}
                  type="button"
                  onClick={() => {
                    setView(v);
                    reset();
                  }}
                  className={`flex-1 px-3 py-1.5 rounded-lg text-sm capitalize transition border ${
                    view === v
                      ? "bg-emerald-500/30 border-emerald-400/50 text-white"
                      : "bg-white/5 border-white/10 text-gray-300 hover:bg-white/10"
                  }`}
                >
                  {v}
                </button>
              ))}
            </div>

            {view === "import" && (
              <input
                type="password"
                value={importKey}
                onChange={(e) => setImportKey(e.target.value)}
                placeholder="Private key (64 hex chars)"
                className={`${field} mb-2`}
                autoComplete="off"
              />
            )}

            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={view === "unlock" ? "Password" : "New password (min 8 chars)"}
              className={field}
              autoComplete="current-password"
              onKeyDown={(e) => e.key === "Enter" && view === "unlock" && handleUnlock()}
            />

            {view !== "unlock" && (
              <input
                type="password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                placeholder="Confirm password"
                className={`${field} mt-2`}
                autoComplete="new-password"
              />
            )}

            <button
              type="button"
              disabled={busy || !password || !secureContext}
              onClick={
                view === "unlock" ? handleUnlock : view === "create" ? handleCreate : handleImport
              }
              className={primary}
            >
              {busy
                ? "Working…"
                : view === "unlock"
                ? "Unlock"
                : view === "create"
                ? "Create wallet"
                : "Import wallet"}
            </button>

            {view === "create" && (
              <p className="text-[11px] text-gray-400 mt-2">
                A new key is generated in your browser. Encrypted with
                AES-256-GCM; password stretched with PBKDF2 (310k iterations).
              </p>
            )}
          </>
        )}

        {unlocked && (
          <>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={lock}
                className="flex-1 px-3 py-2 rounded-lg bg-white/10 hover:bg-white/20 border border-white/20 text-white text-sm transition"
              >
                Lock
              </button>
              <button
                type="button"
                onClick={() => {
                  if (
                    window.confirm(
                      "Remove this wallet from the device? Without your private key backup it cannot be recovered."
                    )
                  ) {
                    removeWallet();
                  }
                }}
                className="flex-1 px-3 py-2 rounded-lg bg-red-500/20 hover:bg-red-500/30 border border-red-400/30 text-red-100 text-sm transition"
              >
                Remove
              </button>
            </div>
            <p className="text-[11px] text-gray-400 mt-3">
              Signing happens in this tab. Auto-locks after 15 minutes idle.
            </p>
            <details className="mt-3">
              <summary className="text-xs text-gray-400 cursor-pointer hover:text-gray-200">
                Export private key
              </summary>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Confirm password to reveal"
                className={`${field} mt-2`}
                autoComplete="off"
              />
              <button
                type="button"
                onClick={handleExport}
                disabled={busy || !password}
                className="w-full mt-2 px-3 py-2 rounded-lg bg-red-500/20 hover:bg-red-500/30 border border-red-400/30 text-red-100 text-sm disabled:opacity-40 transition"
              >
                Reveal
              </button>
            </details>
          </>
        )}

        {(error || notice) && (
          <p
            className={`mt-3 text-xs break-words ${
              notice?.type === "success" ? "text-green-300" : "text-red-300"
            }`}
          >
            {notice?.text || error}
          </p>
        )}
      </div>
    </div>
  );
};

export default WalletPanel;
