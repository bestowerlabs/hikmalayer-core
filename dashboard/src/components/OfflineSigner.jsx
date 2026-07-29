import React, { useState } from "react";

/// Shared offline-signing panel.
///
/// Hikmalayer never accepts a private key over the network, and this UI never
/// asks for one. It shows the exact `hikma-wallet` command plus the canonical
/// message the node will verify; the user signs on an offline machine and
/// pastes back only the public key and signature.
const CopyRow = ({ label, value }) => {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="mb-3">
      <div className="flex items-center justify-between mb-1">
        <span className="text-xs uppercase tracking-wide text-gray-400">
          {label}
        </span>
        <button
          type="button"
          onClick={copy}
          className="text-xs px-2 py-0.5 rounded-md bg-white/10 hover:bg-white/20 text-gray-200 transition"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <code className="block w-full break-all rounded-lg bg-black/40 border border-white/10 p-2 text-[11px] leading-relaxed text-teal-200">
        {value}
      </code>
    </div>
  );
};

const OfflineSigner = ({
  command,
  message,
  publicKey,
  signature,
  onPublicKey,
  onSignature,
  disabled,
}) => (
  <div className="rounded-xl border border-white/10 bg-black/20 p-3 mt-3">
    <p className="text-xs text-gray-400 mb-3">
      🔐 Sign offline — your private key never enters this browser or the node.
    </p>

    <CopyRow label="1. Run offline" value={command} />
    <CopyRow label="2. Message being authorized" value={message} />

    <div className="grid grid-cols-1 gap-2">
      <input
        type="text"
        value={publicKey}
        onChange={(e) => onPublicKey(e.target.value)}
        placeholder="3. public_key from the wallet output"
        disabled={disabled}
        className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-xs placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-teal-500/50 disabled:opacity-50"
      />
      <input
        type="text"
        value={signature}
        onChange={(e) => onSignature(e.target.value)}
        placeholder="4. signature from the wallet output"
        disabled={disabled}
        className="w-full px-3 py-2 rounded-lg bg-white/10 border border-white/20 text-white text-xs placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-teal-500/50 disabled:opacity-50"
      />
    </div>
  </div>
);

export default OfflineSigner;
