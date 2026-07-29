// window.hikmalayer — the provider a dapp talks to.
//
// This runs in the PAGE's world, so treat everything here as public. It holds
// no secrets: it only forwards requests to the isolated content bridge, which
// forwards them to the background worker where the key actually lives. The
// page can ask for a signature; it can never obtain the key, and it cannot
// approve its own request.

(() => {
  const CHANNEL_REQ = "hikmalayer:request";
  const CHANNEL_RES = "hikmalayer:response";
  const CHANNEL_EVENT = "hikmalayer:event";

  const inflight = new Map();
  const listeners = new Map();

  window.addEventListener("message", (event) => {
    if (event.source !== window || !event.data) return;
    const { channel, id, ok, result, error, event: name, payload } = event.data;

    if (channel === CHANNEL_RES && inflight.has(id)) {
      const { resolve, reject } = inflight.get(id);
      inflight.delete(id);
      if (ok) resolve(result);
      else reject(new Error(error || "Request failed"));
      return;
    }

    if (channel === CHANNEL_EVENT) {
      for (const handler of listeners.get(name) ?? []) {
        try {
          handler(payload);
        } catch {
          /* a dapp's handler must not break the provider */
        }
      }
    }
  });

  const provider = {
    isHikmalayerWallet: true,
    /// Chain identity, so a dapp can refuse to talk to the wrong network.
    chain: "hikmalayer",

    request({ method, params } = {}) {
      if (typeof method !== "string") {
        return Promise.reject(new Error("method must be a string"));
      }
      const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
      return new Promise((resolve, reject) => {
        inflight.set(id, { resolve, reject });
        window.postMessage({ channel: CHANNEL_REQ, id, method, params }, window.location.origin);
        // Never leave a dapp hanging forever if the extension goes away.
        setTimeout(() => {
          if (inflight.has(id)) {
            inflight.delete(id);
            reject(new Error("Wallet did not respond"));
          }
        }, 300_000);
      });
    },

    on(name, handler) {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name).add(handler);
      return () => listeners.get(name)?.delete(handler);
    },

    removeListener(name, handler) {
      listeners.get(name)?.delete(handler);
    },

    // Convenience wrappers over `request`.
    connect() {
      return this.request({ method: "hikma_requestAccounts" });
    },
    getAccounts() {
      return this.request({ method: "hikma_accounts" });
    },
    getPublicKey() {
      return this.request({ method: "hikma_getPublicKey" });
    },
    signMessage(message) {
      return this.request({ method: "hikma_signMessage", params: { message } });
    },
  };

  // Freeze so a hostile script on the page cannot swap the provider out for a
  // lookalike that phishes for approvals under our name.
  Object.freeze(provider);
  Object.defineProperty(window, "hikmalayer", {
    value: provider,
    writable: false,
    configurable: false,
    enumerable: true,
  });

  window.dispatchEvent(new Event("hikmalayer#initialized"));
})();
