// Isolated content bridge.
//
// Runs in an isolated world: the page cannot read its variables or call into
// it except through window.postMessage, and it is the ONLY path from a page to
// the background worker. It deliberately does nothing but relay — no keys, no
// decisions, no trust in anything the page says about itself (the origin used
// for permissions is taken by the browser from `sender`, not from here).

const CHANNEL_REQ = "hikmalayer:request";
const CHANNEL_RES = "hikmalayer:response";
const CHANNEL_EVENT = "hikmalayer:event";

// Only these methods may ever be invoked from a web page.
const PAGE_METHODS = new Set([
  "hikma_chainInfo",
  "hikma_accounts",
  "hikma_requestAccounts",
  "hikma_getPublicKey",
  "hikma_signMessage",
]);

window.addEventListener("message", (event) => {
  if (event.source !== window || event.data?.channel !== CHANNEL_REQ) return;

  const { id, method, params } = event.data;
  const reply = (payload) =>
    window.postMessage({ channel: CHANNEL_RES, id, ...payload }, window.location.origin);

  if (!PAGE_METHODS.has(method)) {
    reply({ ok: false, error: `Unsupported method: ${method}` });
    return;
  }

  chrome.runtime
    .sendMessage({ channel: "hikmalayer:page", method, params })
    .then((response) => {
      if (!response) {
        reply({ ok: false, error: "Wallet unavailable" });
        return;
      }
      reply(response);
    })
    .catch((error) => reply({ ok: false, error: error?.message || "Wallet unavailable" }));
});

// Wallet-originated events (lock, disconnect) reach the page as provider events.
chrome.runtime.onMessage.addListener((message) => {
  if (message?.type === "hikmalayer:locked") {
    window.postMessage(
      { channel: CHANNEL_EVENT, event: "lock", payload: null },
      window.location.origin
    );
  }
  if (message?.type === "hikmalayer:accountsChanged") {
    window.postMessage(
      { channel: CHANNEL_EVENT, event: "accountsChanged", payload: message.accounts ?? [] },
      window.location.origin
    );
  }
});
