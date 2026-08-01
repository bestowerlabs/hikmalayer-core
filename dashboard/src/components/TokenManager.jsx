import React, { useCallback, useEffect, useMemo, useState } from "react";
import { getAccountNonce, getTokenBalance, transferTokens } from "../api";
import { useWallet } from "../hooks/useWallet";
import { useActiveSigner } from "../hooks/useActiveSigner";
import OfflineSigner from "./OfflineSigner";
import { HKM_DECIMALS, formatUnits, getActiveChainId, parseUnits, scoped } from "../lib/hts";

/// Send the native coin.
///
/// Amounts here are HKM, not base units. The chain counts in base units and
/// HKM has six decimals, so the two differ by a factor of a million — a panel
/// that blurs them will move a millionth of what the user meant to send.
/// Everything below parses to base units through BigInt and puts the exact
/// digits on the wire.
const TokenManager = ({ refreshTrigger, onUpdate }) => {
  const { account } = useWallet();
  const { canSign, sign, publicKey: signerPublicKey } = useActiveSigner();

  const [to, setTo] = useState("");
  const [amount, setAmount] = useState("");
  const [publicKey, setPublicKey] = useState("");
  const [signature, setSignature] = useState("");
  const [nonce, setNonce] = useState(null);
  const [balance, setBalance] = useState(null);
  const [lookupAddress, setLookupAddress] = useState("");
  const [lookupBalance, setLookupBalance] = useState(null);
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
      balanceResult.status === "fulfilled"
        ? (balanceResult.value.data?.balance ?? null)
        : null
    );
    setNonce(
      nonceResult.status === "fulfilled"
        ? (nonceResult.value.data?.next_nonce ?? null)
        : null
    );
  }, [account]);

  useEffect(() => {
    refreshAccount();
  }, [refreshAccount, refreshTrigger]);

  useEffect(() => {
    if (account) setLookupAddress((current) => current || account);
  }, [account]);

  // Base units, or an error the user can act on.
  const parsed = useMemo(() => {
    try {
      return { units: parseUnits(amount, HKM_DECIMALS), error: null };
    } catch (error) {
      return { units: 0n, error: error.message };
    }
  }, [amount]);

  // Scoped to this network, exactly as the node will reconstruct it.
  const canonicalMessage = scoped(
    getActiveChainId() ?? "<network>",
    `hikmalayer-transfer:${account || "<from>"}:${to.trim() || "<to>"}` +
      `:${parsed.units}:${nonce ?? "<nonce>"}`
  );

  // The CLI scopes what it signs to HIKMALAYER_CHAIN_ID; without it the
  // signature is for the dev network and this node refuses it.
  const signingCommand =
    `HIKMALAYER_CHAIN_ID=${getActiveChainId() ?? "<network>"} ` +
    `hikma-wallet sign-transfer ${account || "<from>"} ${to.trim() || "<to>"} ` +
    `${parsed.units} ${nonce ?? "<nonce>"} <PRIVATE_KEY>`;

  const sendingToSelf = !!account && to.trim() === account;
  const exceedsBalance = balance !== null && parsed.units > BigInt(balance);

  const ready =
    account &&
    to.trim() &&
    !sendingToSelf &&
    parsed.units > 0n &&
    !exceedsBalance &&
    nonce !== null &&
    (canSign || (publicKey.trim() && signature.trim()));

  const submit = async (event) => {
    event.preventDefault();
    if (!ready) return;
    setBusy(true);
    setMessage(null);
    try {
      const signedBy = canSign
        ? { public_key: signerPublicKey, signature: await sign(canonicalMessage) }
        : { public_key: publicKey.trim(), signature: signature.trim() };
      const res = await transferTokens({
        from: account,
        to: to.trim(),
        // Decimal string: a base-unit amount can exceed 2^53, and the
        // signature covers these exact digits.
        amount: parsed.units.toString(),
        nonce,
        ...signedBy,
      });
      const ok = res.data?.status === "success";
      setMessage({
        type: ok ? "success" : "error",
        text:
          res.data?.message ||
          (ok ? `Sent ${formatUnits(parsed.units, HKM_DECIMALS)} HKM` : "Transfer failed"),
      });
      if (ok) {
        setTo("");
        setAmount("");
        setSignature("");
        await refreshAccount();
        onUpdate?.();
      }
    } catch (error) {
      setMessage({
        type: "error",
        text: error.response?.data?.message || error.message || "Transfer failed",
      });
    } finally {
      setBusy(false);
    }
  };

  const checkBalance = async (event) => {
    event.preventDefault();
    const address = lookupAddress.trim();
    if (!address) return;
    try {
      const res = await getTokenBalance(address);
      setLookupBalance(res.data?.balance ?? 0);
    } catch {
      setLookupBalance(null);
      setMessage({ type: "error", text: "Could not fetch that balance." });
    }
  };

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-yellow-500/10 to-orange-500/10" />

      <div className="relative z-10">
        <div className="flex items-center mb-5">
          <div className="p-2 rounded-xl bg-gradient-to-r from-yellow-500/20 to-orange-500/20 mr-3">
            <span className="text-2xl">💰</span>
          </div>
          <div>
            <h2 className="text-xl font-bold text-white">Send HKM</h2>
            <p className="text-sm text-gray-300">
              The native coin · 6 decimals
            </p>
          </div>
        </div>

        {account && (
          <div className="mb-5 p-4 rounded-xl bg-gradient-to-r from-green-500/20 to-emerald-500/20 border border-green-500/30">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-green-300">Your balance</div>
                <div className="text-2xl font-bold text-white">
                  {balance === null ? "—" : formatUnits(balance, HKM_DECIMALS)}{" "}
                  <span className="text-base font-medium text-green-200">HKM</span>
                </div>
              </div>
              <div className="text-4xl">💎</div>
            </div>
          </div>
        )}

        <form onSubmit={submit} className="space-y-3">
          <div>
            <label className="block text-xs text-gray-400 mb-1">To address</label>
            <input
              type="text"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              placeholder="hkm…"
              disabled={busy}
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/50"
            />
            {sendingToSelf && (
              <p className="text-xs text-amber-300 mt-1">
                That is your own address.
              </p>
            )}
          </div>

          <div>
            <label className="block text-xs text-gray-400 mb-1">Amount (HKM)</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                placeholder="0.000000"
                disabled={busy}
                className="flex-1 px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/50"
              />
              {balance !== null && (
                <button
                  type="button"
                  onClick={() => setAmount(formatUnits(balance, HKM_DECIMALS))}
                  disabled={busy}
                  className="px-4 py-2 rounded-lg bg-white/10 hover:bg-white/20 border border-white/20 text-white text-sm transition"
                >
                  Max
                </button>
              )}
            </div>
            {parsed.error && (
              <p className="text-xs text-red-300 mt-1">{parsed.error}</p>
            )}
            {!parsed.error && parsed.units > 0n && (
              <p className="text-xs text-gray-400 mt-1">
                = {parsed.units.toString()} base units
              </p>
            )}
            {exceedsBalance && (
              <p className="text-xs text-red-300 mt-1">
                More than your balance. A transaction fee is charged on top of
                the amount, so leave a little headroom.
              </p>
            )}
          </div>

          {!account && (
            <p className="text-xs text-amber-300">
              Connect a wallet above to send.
            </p>
          )}

          {account && !canSign && parsed.units > 0n && to.trim() && (
            <OfflineSigner
              command={signingCommand}
              message={canonicalMessage}
              publicKey={publicKey}
              signature={signature}
              onPublicKey={setPublicKey}
              onSignature={setSignature}
              disabled={busy}
            />
          )}

          <button
            type="submit"
            disabled={!ready || busy}
            className="w-full px-4 py-2.5 rounded-xl bg-gradient-to-r from-indigo-500 to-purple-600 text-white font-semibold text-sm disabled:opacity-40 disabled:cursor-not-allowed hover:from-indigo-400 hover:to-purple-500 transition"
          >
            {busy ? "Sending…" : canSign ? "Sign & send" : "Send"}
          </button>

          {message && (
            <p
              className={`text-xs break-words ${
                message.type === "success" ? "text-green-300" : "text-red-300"
              }`}
            >
              {message.text}
            </p>
          )}
        </form>

        <form
          onSubmit={checkBalance}
          className="mt-6 pt-5 border-t border-white/10 space-y-2"
        >
          <label className="block text-xs text-gray-400">
            Look up any balance
          </label>
          <div className="flex gap-2">
            <input
              type="text"
              value={lookupAddress}
              onChange={(e) => setLookupAddress(e.target.value)}
              placeholder="hkm…"
              className="flex-1 px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-green-500/50"
            />
            <button
              type="submit"
              className="px-4 py-2 rounded-lg bg-gradient-to-r from-green-500 to-teal-600 text-white text-sm font-medium hover:from-green-400 hover:to-teal-500 transition"
            >
              Check
            </button>
          </div>
          {lookupBalance !== null && (
            <p className="text-sm text-white">
              {formatUnits(lookupBalance, HKM_DECIMALS)}{" "}
              <span className="text-gray-400">HKM</span>
            </p>
          )}
        </form>
      </div>
    </div>
  );
};

export default TokenManager;
