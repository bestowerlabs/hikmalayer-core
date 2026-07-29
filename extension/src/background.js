// Hikmalayer Wallet — background service worker.
//
// THIS is the security boundary. The private key exists only here, in the
// extension's own context. A web page (and therefore any XSS in a web page)
// cannot read this memory, cannot call these functions directly, and cannot
// reach chrome.storage. Pages may only send messages asking to connect or to
// sign, and every such request is approved by the user in extension UI that
// the page cannot draw over, spoof, or auto-click.
//
// Crypto is the same implementation the dashboard and CLI use, proven
// byte-for-byte identical to the Rust signer.
import {
  createSessionKey,
  decryptVault,
  deriveAddress,
  derivePublicKey,
  encryptVault,
  generatePrivateKey,
  isValidPrivateKey,
  protectKey,
  signMessageFromBytes,
  withProtectedKey,
} from "../../dashboard/src/lib/wallet.js";

const VAULT_KEY = "hikmalayer.vault.v1";
const SITES_KEY = "hikmalayer.sites.v1";
const AUTO_LOCK_MS = 15 * 60 * 1000;

// ---- Session state (memory only; dies with the worker) ------------------
let sessionKey = null;
let protectedKey = null;
let lastActivity = 0;
let lockTimer = null;

/// Pending user decisions, keyed by id. Each holds the promise settlers for
/// the page request that is waiting.
const pending = new Map();

const isUnlocked = () => !!sessionKey && !!protectedKey;

function touch() {
  lastActivity = Date.now();
  if (lockTimer) clearTimeout(lockTimer);
  lockTimer = setTimeout(() => {
    if (Date.now() - lastActivity >= AUTO_LOCK_MS) lock();
  }, AUTO_LOCK_MS + 1000);
}

function lock() {
  sessionKey = null;
  protectedKey = null;
  if (lockTimer) clearTimeout(lockTimer);
  lockTimer = null;
  broadcastToPages({ type: "hikmalayer:locked" });
  chrome.runtime.sendMessage({ type: "wallet:state-changed" }).catch(() => {});
}

// ---- Storage helpers ----------------------------------------------------
async function getVault() {
  const stored = await chrome.storage.local.get(VAULT_KEY);
  return stored[VAULT_KEY] ?? null;
}

async function setVault(vault) {
  await chrome.storage.local.set({ [VAULT_KEY]: vault });
}

async function getSites() {
  const stored = await chrome.storage.local.get(SITES_KEY);
  return stored[SITES_KEY] ?? {};
}

async function setSiteConnected(origin, connected) {
  const sites = await getSites();
  if (connected) sites[origin] = { connectedAt: Date.now() };
  else delete sites[origin];
  await chrome.storage.local.set({ [SITES_KEY]: sites });
}

// ---- Approval windows ---------------------------------------------------
//
// Approvals render in an extension page. A website cannot script this window,
// cannot read it, and cannot press its buttons.
async function openApproval(requestId) {
  const url = chrome.runtime.getURL(`popup.html?request=${encodeURIComponent(requestId)}`);
  await chrome.windows.create({ url, type: "popup", width: 420, height: 640 });
}

function settle(requestId, result, error) {
  const entry = pending.get(requestId);
  if (!entry) return false;
  pending.delete(requestId);
  if (error) entry.reject(error);
  else entry.resolve(result);
  return true;
}

function requestApproval(request) {
  return new Promise((resolve, reject) => {
    pending.set(request.id, { ...request, resolve, reject });
    openApproval(request.id).catch(() => {
      pending.delete(request.id);
      reject(new Error("Could not open the approval window"));
    });
  });
}

async function broadcastToPages(message) {
  const tabs = await chrome.tabs.query({});
  for (const tab of tabs) {
    if (tab.id != null) {
      chrome.tabs.sendMessage(tab.id, message).catch(() => {});
    }
  }
}

// ---- Signing ------------------------------------------------------------
async function signWithVault(message) {
  if (!isUnlocked()) throw new Error("Wallet is locked");
  touch();
  return withProtectedKey(sessionKey, protectedKey, (keyBytes) =>
    signMessageFromBytes(message, keyBytes)
  );
}

// ---- Page-facing API (via the content bridge) ---------------------------
//
// `origin` is supplied by the browser (sender.origin), never by the page, so
// a site cannot claim to be another site.
async function handlePageRequest(method, params, origin) {
  const vault = await getVault();
  const sites = await getSites();
  const connected = !!sites[origin];

  switch (method) {
    case "hikma_chainInfo":
      return { name: "Hikmalayer", ticker: "HKM", decimals: 6, addressPrefix: "hkm" };

    case "hikma_accounts":
      // Never reveals anything to an unconnected site.
      return connected && vault ? [vault.address] : [];

    case "hikma_requestAccounts": {
      if (!vault) throw new Error("No wallet set up. Open the Hikmalayer Wallet extension first.");
      if (connected) return [vault.address];
      await requestApproval({
        id: crypto.randomUUID(),
        kind: "connect",
        origin,
        address: vault.address,
      });
      await setSiteConnected(origin, true);
      return [vault.address];
    }

    case "hikma_getPublicKey": {
      if (!connected || !vault) throw new Error("Not connected");
      return vault.publicKey;
    }

    case "hikma_signMessage": {
      if (!connected || !vault) throw new Error("Not connected");
      const message = params?.message;
      if (typeof message !== "string" || !message) throw new Error("message is required");
      if (message.length > 2000) throw new Error("message is too long");
      if (!isUnlocked()) {
        // Let the user unlock as part of approving.
        await requestApproval({
          id: crypto.randomUUID(),
          kind: "unlock",
          origin,
          address: vault.address,
        });
      }
      await requestApproval({
        id: crypto.randomUUID(),
        kind: "sign",
        origin,
        address: vault.address,
        message,
      });
      const signature = await signWithVault(message);
      return { signature, publicKey: vault.publicKey, address: vault.address };
    }

    default:
      throw new Error(`Unsupported method: ${method}`);
  }
}

// ---- Popup-facing API ---------------------------------------------------
async function handlePopupRequest(message) {
  switch (message.type) {
    case "wallet:state": {
      const vault = await getVault();
      return {
        hasWallet: !!vault,
        unlocked: isUnlocked(),
        address: vault?.address ?? null,
        publicKey: vault?.publicKey ?? null,
        sites: Object.keys(await getSites()),
      };
    }

    case "wallet:create": {
      const privateKey = generatePrivateKey();
      const vault = await encryptVault(privateKey, message.password);
      await setVault(vault);
      sessionKey = await createSessionKey();
      protectedKey = await protectKey(sessionKey, privateKey);
      touch();
      return { address: vault.address, privateKey };
    }

    case "wallet:import": {
      if (!isValidPrivateKey(message.privateKey)) {
        throw new Error("That is not a valid 32-byte private key");
      }
      const vault = await encryptVault(message.privateKey, message.password);
      await setVault(vault);
      sessionKey = await createSessionKey();
      protectedKey = await protectKey(sessionKey, message.privateKey);
      touch();
      return { address: vault.address };
    }

    case "wallet:unlock": {
      const vault = await getVault();
      if (!vault) throw new Error("No wallet on this device");
      const privateKey = await decryptVault(vault, message.password);
      sessionKey = await createSessionKey();
      protectedKey = await protectKey(sessionKey, privateKey);
      touch();
      return { address: vault.address };
    }

    case "wallet:lock":
      lock();
      return { ok: true };

    case "wallet:export": {
      const vault = await getVault();
      if (!vault) throw new Error("No wallet on this device");
      // Requires the password again even while unlocked.
      return { privateKey: await decryptVault(vault, message.password) };
    }

    case "wallet:remove": {
      lock();
      await chrome.storage.local.remove([VAULT_KEY, SITES_KEY]);
      return { ok: true };
    }

    case "wallet:disconnect-site":
      await setSiteConnected(message.origin, false);
      return { ok: true };

    case "request:get": {
      const entry = pending.get(message.id);
      if (!entry) return null;
      const { id, kind, origin, address, message: signMessage } = entry;
      return { id, kind, origin, address, message: signMessage };
    }

    case "request:approve": {
      const entry = pending.get(message.id);
      if (!entry) throw new Error("This request has expired");
      if (entry.kind === "unlock") {
        const vault = await getVault();
        const privateKey = await decryptVault(vault, message.password);
        sessionKey = await createSessionKey();
        protectedKey = await protectKey(sessionKey, privateKey);
        touch();
      }
      settle(message.id, true);
      return { ok: true };
    }

    case "request:reject":
      settle(message.id, null, new Error("User rejected the request"));
      return { ok: true };

    default:
      throw new Error(`Unknown popup message: ${message.type}`);
  }
}

// ---- Message routing ----------------------------------------------------
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  (async () => {
    try {
      // Requests relayed from a web page by the content bridge.
      if (message?.channel === "hikmalayer:page") {
        const origin = sender.origin ?? new URL(sender.url ?? "").origin;
        const result = await handlePageRequest(message.method, message.params, origin);
        sendResponse({ ok: true, result });
        return;
      }
      // Requests from our own popup/approval pages.
      const result = await handlePopupRequest(message);
      sendResponse({ ok: true, result });
    } catch (error) {
      sendResponse({ ok: false, error: error?.message || String(error) });
    }
  })();
  return true; // keep the channel open for the async response
});

// A closing approval window means "rejected" — never leave a page hanging.
chrome.windows.onRemoved.addListener(() => {
  for (const id of [...pending.keys()]) {
    settle(id, null, new Error("User rejected the request"));
  }
});

// Expose derivation helpers for the popup's convenience (public data only).
export { deriveAddress, derivePublicKey };
