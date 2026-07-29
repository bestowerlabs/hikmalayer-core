import React from "react";
import { safeText } from "../lib/sanitize";

/// Signature confirmation.
///
/// Nothing is ever signed without this dialog. It shows the EXACT canonical
/// message the chain will verify — not a friendly summary — because that
/// string is what the signature commits to. If a page were ever compromised,
/// a signing attempt becomes visible and refusable instead of silent.
const SignConfirm = ({ request, address, onApprove, onReject }) => {
  if (!request) return null;

  // The message is untrusted display data: strip anything that could hide or
  // reorder what the user is about to authorize.
  const shown = safeText(request.message, 400);
  const altered = shown !== request.message;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm">
      <div className="w-full max-w-lg rounded-2xl border border-white/20 bg-slate-900 p-6 shadow-2xl">
        <h3 className="text-lg font-bold text-white mb-1 flex items-center gap-2">
          <span>✍️</span> Confirm signature
        </h3>
        <p className="text-xs text-gray-400 mb-4">
          Signing as <span className="text-emerald-200">{address}</span>
        </p>

        <p className="text-xs uppercase tracking-wide text-gray-400 mb-1">
          Exact message to be signed
        </p>
        <code className="block max-h-48 overflow-y-auto break-all rounded-lg bg-black/50 border border-white/10 p-3 text-[11px] leading-relaxed text-teal-200">
          {shown}
        </code>

        {altered && (
          <p className="mt-2 text-[11px] text-amber-300">
            ⚠️ Hidden or text-reordering characters were removed for display.
            Do not approve unless you understand this request.
          </p>
        )}

        <p className="mt-3 text-[11px] text-gray-400">
          Approve only if this matches the action you just requested.
        </p>

        <div className="mt-4 flex gap-2">
          <button
            type="button"
            onClick={onReject}
            className="flex-1 px-4 py-2.5 rounded-xl bg-white/10 hover:bg-white/20 border border-white/20 text-white text-sm transition"
          >
            Reject
          </button>
          <button
            type="button"
            onClick={onApprove}
            autoFocus
            className="flex-1 px-4 py-2.5 rounded-xl bg-gradient-to-r from-emerald-500 to-teal-500 hover:from-emerald-400 hover:to-teal-400 text-white font-semibold text-sm transition"
          >
            Approve &amp; sign
          </button>
        </div>
      </div>
    </div>
  );
};

export default SignConfirm;
