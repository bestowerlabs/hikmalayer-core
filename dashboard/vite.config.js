import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

/// Keep the CSP in step with the node the dashboard is built to talk to.
///
/// `connect-src` is pinned deliberately — it is what stops an injected script
/// exfiltrating to an attacker's host. But a build pointed at a node its own
/// CSP forbids fails silently and looks exactly like the node being down, so
/// the configured origin is added to the policy here rather than left to be
/// remembered by hand.
const syncCspWithApiBase = (apiBase) => ({
  name: "hikmalayer-csp-connect-src",
  transformIndexHtml(html) {
    if (!apiBase) return html;
    // Anchored on `connect-src 'self'` so it cannot match the prose in the
    // surrounding HTML comment, which also mentions the directive by name.
    const directive = /(connect-src 'self'[^;]*);/;
    if (!directive.test(html)) {
      throw new Error(
        "Could not find the connect-src directive in index.html; VITE_API_BASE would be blocked by the CSP."
      );
    }
    return html.replace(directive, (match, sources) =>
      sources.includes(apiBase) ? match : `${sources.trimEnd()} ${apiBase};`
    );
  },
});

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const apiBase = String(env.VITE_API_BASE || "").replace(/\/+$/, "");
  return {
    plugins: [react(), syncCspWithApiBase(apiBase)],
  };
});
