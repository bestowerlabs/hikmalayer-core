import React, { useState } from "react";
import { useAuthenticatedApi } from "../hooks/useAuthenticatedApi";
import { issueCredential, revokeCredential, getCredential, getCredentialProof } from "../api";

/// Verifiable credentials — distinct from /certificates/*. A credential is
/// issued to a subject, can be looked up and cryptographically proven, and
/// can be revoked by its issuer.
const CredentialManager = () => {
  const { isConnected } = useAuthenticatedApi();

  const [issueForm, setIssueForm] = useState({ subject: "", claim: "" });
  const [lookupId, setLookupId] = useState("");
  const [credential, setCredential] = useState(null);
  const [proof, setProof] = useState(null);
  const [message, setMessage] = useState(null);
  const [busy, setBusy] = useState(false);

  const issue = async (event) => {
    event.preventDefault();
    setBusy(true);
    setMessage(null);
    try {
      const res = await issueCredential(issueForm);
      const ok = res.data?.status === "success";
      setMessage({
        type: ok ? "success" : "error",
        text: res.data?.message || (ok ? "Credential issued" : "Issue failed"),
      });
      if (ok) setIssueForm({ subject: "", claim: "" });
    } catch (err) {
      setMessage({
        type: "error",
        text: err.response?.data?.message || "Issue failed — requires authentication.",
      });
    } finally {
      setBusy(false);
    }
  };

  const revoke = async (id) => {
    try {
      const res = await revokeCredential({ id });
      setMessage({
        type: res.data?.status === "success" ? "success" : "error",
        text: res.data?.message || "Revoked",
      });
      if (lookupId === id) lookup();
    } catch (err) {
      setMessage({ type: "error", text: err.response?.data?.message || "Revoke failed." });
    }
  };

  const lookup = async (event) => {
    event?.preventDefault();
    const id = lookupId.trim();
    if (!id) return;
    setBusy(true);
    try {
      const [credRes, proofRes] = await Promise.allSettled([
        getCredential(id),
        getCredentialProof(id),
      ]);
      setCredential(credRes.status === "fulfilled" ? credRes.value.data : null);
      setProof(proofRes.status === "fulfilled" ? proofRes.value.data : null);
      if (credRes.status !== "fulfilled") {
        setMessage({ type: "error", text: "Credential not found." });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
        <div className="absolute inset-0 bg-gradient-to-br from-rose-500/10 to-pink-500/10" />
        <div className="relative z-10">
          <div className="flex items-center mb-5">
            <div className="p-2 rounded-xl bg-gradient-to-r from-rose-500/20 to-pink-500/20 mr-3">
              <span className="text-2xl">🎖️</span>
            </div>
            <div>
              <h2 className="text-xl font-bold text-white">Issue Credential</h2>
              <p className="text-sm text-gray-300">Requires authentication</p>
            </div>
          </div>
          <form onSubmit={issue} className="space-y-3">
            <input
              type="text"
              value={issueForm.subject}
              onChange={(e) => setIssueForm((f) => ({ ...f, subject: e.target.value }))}
              placeholder="Subject (hkm…)"
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-rose-500/50"
            />
            <input
              type="text"
              value={issueForm.claim}
              onChange={(e) => setIssueForm((f) => ({ ...f, claim: e.target.value }))}
              placeholder="Claim / description"
              className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-rose-500/50"
            />
            <button
              type="submit"
              disabled={busy || !isConnected}
              className="w-full px-4 py-2 rounded-lg bg-gradient-to-r from-rose-500 to-pink-600 text-white text-sm font-medium hover:from-rose-400 hover:to-pink-500 transition disabled:opacity-40"
            >
              {busy ? "Issuing…" : "Issue"}
            </button>
            {message && (
              <p className={`text-sm ${message.type === "success" ? "text-green-400" : "text-red-400"}`}>
                {message.text}
              </p>
            )}
          </form>
        </div>
      </div>

      <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
        <div className="absolute inset-0 bg-gradient-to-br from-rose-500/10 to-pink-500/10" />
        <div className="relative z-10">
          <div className="flex items-center mb-5">
            <div className="p-2 rounded-xl bg-gradient-to-r from-rose-500/20 to-pink-500/20 mr-3">
              <span className="text-2xl">🔍</span>
            </div>
            <div>
              <h2 className="text-xl font-bold text-white">Verify / Revoke</h2>
              <p className="text-sm text-gray-300">Look up by credential ID</p>
            </div>
          </div>
          <form onSubmit={lookup} className="flex gap-2 mb-4">
            <input
              type="text"
              value={lookupId}
              onChange={(e) => setLookupId(e.target.value)}
              placeholder="Credential ID"
              className="flex-1 px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-sm placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-rose-500/50"
            />
            <button
              type="submit"
              className="px-4 py-2 rounded-lg bg-gradient-to-r from-rose-500 to-pink-600 text-white text-sm font-medium hover:from-rose-400 hover:to-pink-500 transition"
            >
              Check
            </button>
          </form>

          {credential && (
            <div className="space-y-2 text-sm mb-3">
              {Object.entries(credential).map(([key, value]) => (
                <div key={key} className="flex justify-between border-b border-white/10 pb-1">
                  <span className="text-gray-400">{key}</span>
                  <span className="text-white font-mono truncate ml-2">{String(value)}</span>
                </div>
              ))}
              <button
                onClick={() => revoke(credential.id ?? lookupId)}
                disabled={!isConnected}
                className="mt-2 px-3 py-1.5 rounded-lg bg-red-500/20 border border-red-500/40 text-red-300 text-xs font-medium hover:bg-red-500/30 transition disabled:opacity-40"
              >
                Revoke
              </button>
            </div>
          )}

          {proof && (
            <details className="mt-3">
              <summary className="text-xs text-gray-400 cursor-pointer">Cryptographic proof</summary>
              <pre className="mt-2 text-xs text-gray-300 font-mono overflow-x-auto">
                {JSON.stringify(proof, null, 2)}
              </pre>
            </details>
          )}
        </div>
      </div>
    </div>
  );
};

export default CredentialManager;
