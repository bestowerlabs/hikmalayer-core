// Hikmalayer Wallet — background service worker.
//
// THIS is the security boundary. The private keys exist only here, in the
// extension's own context. A web page (and therefore any XSS in a web page)
// cannot read this memory, cannot call these functions directly, and cannot
// reach chrome.storage. Pages may only send messages asking to connect or to
// sign, and every such request is approved by the user in extension UI that
// the page cannot draw over, spoof, or auto-click.
//
// Crypto is the same implementation the dashboard and CLI use, proven
// byte-for-byte identical to the Rust signer.
import {
  activeAccountIndex,
  createSessionKey,
  decryptKeyring,
  deriveAddress,
  derivePublicKey,
  encryptKeyring,
  generatePrivateKey,
  isValidPrivateKey,
  normalizeHex,
  protectKey,
  signMessageFromBytes,
  vaultAccounts,
  withProtectedKey,
} from "../../dashboard/src/lib/wallet.js";
import {
  hybridIdentityFromBytes,
  signHybridFromBytes,
} from "../../dashboard/src/lib/hybrid.js";
import { hexToBytes } from "@noble/hashes/utils";

const VAULT_KEY = "hikmalayer.vault.v1"; // storage slot; holds a v1 or v2 vault
const SITES_KEY = "hikmalayer.sites.v1";
const NETWORK_KEY = "hikmalayer.network.v1";
/// Which of each key's two accounts this wallet operates as: the classical
/// `hkm…` one, or the quantum-ready `hkq…` one that also requires an ML-DSA
/// signature. A preference, not a secret — but it decides which address a
/// site sees, so it is stored rather than asked each time.
const SCHEME_KEY = "hikmalayer.scheme.v1";
const AUTO_LOCK_MS = 15 * 60 * 1000;

/// Where the popup reads balances from. Signing never needs a node, so this
/// is a convenience only — but it must be editable, because a wallet pinned
/// to one hardcoded host is useless against any real deployment.
const DEFAULT_NODE_URL = "http://127.0.0.1:3000";

// ---- Session state (memory only; dies with the worker) ------------------
let sessionKey = null;
/// One protected key per account, in account order. Each is the private key
/// encrypted under a non-extractable session key, decrypted only for the
/// instant of signing.
let protectedKeys = [];
/// The hybrid identity of each account, in account order. Public material,
/// derived on unlock (~3 ms of ML-DSA keygen per account) and held only in
/// memory: a key already determines it, so there is nothing to persist.
let hybridIdentities = [];
let lastActivity = 0;
let lockTimer = null;

/// Pending user decisions, keyed by id. Each holds the promise settlers for
/// the page request that is waiting, plus the id of the window showing it.
const pending = new Map();

const isUnlocked = () => !!sessionKey && protectedKeys.length > 0;

function touch() {
  lastActivity = Date.now();
  if (lockTimer) clearTimeout(lockTimer);
  lockTimer = setTimeout(() => {
    if (Date.now() - lastActivity >= AUTO_LOCK_MS) lock();
  }, AUTO_LOCK_MS + 1000);
}

function lock() {
  sessionKey = null;
  protectedKeys = [];
  hybridIdentities = [];
  if (lockTimer) clearTimeout(lockTimer);
  lockTimer = null;
  broadcastToPages({ type: "hikmalayer:locked" });
  notifyPopup();
}

/// Tell any open popup that state changed. A closed popup means no receiver,
/// which is not an error worth surfacing.
function notifyPopup() {
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

async function getScheme() {
  const stored = await chrome.storage.local.get(SCHEME_KEY);
  return stored[SCHEME_KEY] === "hybrid" ? "hybrid" : "classical";
}

async function setScheme(value) {
  const scheme = value === "hybrid" ? "hybrid" : "classical";
  await chrome.storage.local.set({ [SCHEME_KEY]: scheme });
  return scheme;
}

async function getNodeUrl() {
  const stored = await chrome.storage.local.get(NETWORK_KEY);
  return stored[NETWORK_KEY]?.nodeUrl || DEFAULT_NODE_URL;
}

/// Accept only an http(s) origin, normalized and without a trailing slash.
/// The popup fetches this URL, so a `javascript:` or `data:` value must never
/// get as far as being stored.
function normalizeNodeUrl(value) {
  const text = String(value ?? "").trim();
  if (!text) return DEFAULT_NODE_URL;
  let url;
  try {
    url = new URL(text);
  } catch {
    throw new Error("That is not a valid URL");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Node URL must start with http:// or https://");
  }
  return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
}

// ---- Accounts -----------------------------------------------------------

/// The account a site sees and signs with. Sites are never given the whole
/// list: switching accounts is the user's decision, made in extension UI.
function activeAccount(vault) {
  const accounts = vaultAccounts(vault);
  if (accounts.length === 0) return null;
  return accounts[activeAccountIndex(vault)] ?? accounts[0];
}

/// The account a site actually deals with, under the selected scheme.
///
/// One private key controls two DIFFERENT accounts with separate balances:
/// `hkm…` (ECDSA) and `hkq…` (ECDSA + ML-DSA-65). The hybrid one depends on
/// the ML-DSA key, which only exists while unlocked — so a locked wallet in
/// hybrid mode reports no address rather than silently falling back to the
/// classical one and having a site pay the wrong account.
function activeIdentity(vault, scheme) {
  const account = activeAccount(vault);
  if (!account) return null;
  if (scheme !== "hybrid") return { ...account, scheme: "classical" };
  const hybrid = hybridIdentities[activeAccountIndex(vault)];
  if (!hybrid) return null;
  return {
    ...account,
    scheme: "hybrid",
    address: hybrid.address,
    publicKey: hybrid.publicKey,
    pqPublicKey: hybrid.pqPublicKey,
  };
}

/// Load private keys into session memory, each individually protected.
async function openSession(privateKeys) {
  sessionKey = await createSessionKey();
  protectedKeys = [];
  hybridIdentities = [];
  for (const key of privateKeys) {
    protectedKeys.push(await protectKey(sessionKey, key));
    // Derived from the raw bytes, then wiped: the hex string is never a
    // second long-lived copy of the key.
    const bytes = hexToBytes(normalizeHex(key));
    try {
      hybridIdentities.push(hybridIdentityFromBytes(bytes));
    } finally {
      bytes.fill(0);
    }
  }
  touch();
}

/// Re-encrypt the keyring after adding or removing an account. Requires the
/// password: the plaintext keys are never held outside a signing instant, so
/// there is nothing to re-encrypt from without it.
async function rewriteKeyring(password, mutate) {
  const vault = await getVault();
  const keys = await decryptKeyring(vault, password);
  const accounts = vaultAccounts(vault);
  const next = mutate({ keys: [...keys], accounts: [...accounts], vault });
  const updated = await encryptKeyring(next.keys, password, {
    labels: next.labels,
    activeIndex: next.activeIndex,
  });
  await setVault(updated);
  await openSession(next.keys);
  notifyPopup();
  broadcastAccountsChanged(activeIdentity(updated, await getScheme())?.address ?? null);
  return updated;
}

// ---- Approval windows ---------------------------------------------------
//
// Approvals render in an extension page. A website cannot script this window,
// cannot read it, and cannot press its buttons.
function requestApproval(request) {
  return new Promise((resolve, reject) => {
    pending.set(request.id, { ...request, resolve, reject, windowId: null });
    const url = chrome.runtime.getURL(
      `popup.html?request=${encodeURIComponent(request.id)}`
    );
    chrome.windows
      .create({ url, type: "popup", width: 420, height: 660 })
      .then((window) => {
        const entry = pending.get(request.id);
        // Remember which window is showing this request, so closing an
        // unrelated window cannot cancel it. (Settled already? Then the
        // entry is gone and there is nothing to record.)
        if (entry) entry.windowId = window?.id ?? null;
      })
      .catch(() => {
        pending.delete(request.id);
        reject(new Error("Could not open the approval window"));
      });
  });
}

function settle(requestId, result, error) {
  const entry = pending.get(requestId);
  if (!entry) return false;
  pending.delete(requestId);
  if (error) entry.reject(error);
  else entry.resolve(result);
  return true;
}

/// Send to every tab. Only for messages that carry no account data — `lock`
/// tells a page nothing it could not already infer.
async function broadcastToPages(message) {
  const tabs = await chrome.tabs.query({});
  for (const tab of tabs) {
    if (tab.id != null) {
      chrome.tabs.sendMessage(tab.id, message).catch(() => {});
    }
  }
}

/// Announce the active account only to sites the user has connected. A site
/// that was never granted access must not learn an address by sitting in a
/// background tab while the user switches accounts.
async function broadcastAccountsChanged(address) {
  const sites = await getSites();
  const tabs = await chrome.tabs.query({});
  for (const tab of tabs) {
    if (tab.id == null || !tab.url) continue;
    let origin;
    try {
      origin = new URL(tab.url).origin;
    } catch {
      continue;
    }
    if (!sites[origin]) continue;
    chrome.tabs
      .sendMessage(tab.id, {
        type: "hikmalayer:accountsChanged",
        accounts: address ? [address] : [],
      })
      .catch(() => {});
  }
}

// ---- Signing ------------------------------------------------------------
async function signWithVault(message, accountIndex, scheme = "classical") {
  if (!isUnlocked()) throw new Error("Wallet is locked");
  const protectedKey = protectedKeys[accountIndex];
  if (!protectedKey) throw new Error("That account is not available");
  touch();
  // A hybrid account gets both signatures in one pass over the key bytes,
  // so the key is decrypted once rather than twice.
  return withProtectedKey(sessionKey, protectedKey, (keyBytes) =>
    scheme === "hybrid"
      ? signHybridFromBytes(message, keyBytes)
      : { signature: signMessageFromBytes(message, keyBytes) }
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
  const scheme = await getScheme();
  const account = activeIdentity(vault, scheme);

  switch (method) {
    case "hikma_chainInfo":
      return {
        name: "Hikmalayer",
        ticker: "HKM",
        decimals: 6,
        addressPrefix: "hkm",
        // Both account types exist on one chain; `scheme` tells a dApp which
        // one it is talking to, so it knows whether to expect post-quantum
        // fields on the signature it gets back.
        addressPrefixes: ["hkm", "hkq"],
        scheme,
      };

    case "hikma_accounts":
      // Never reveals anything to an unconnected site.
      return connected && account ? [account.address] : [];

    case "hikma_requestAccounts": {
      if (!account) {
        throw new Error(
          scheme === "hybrid"
            ? "Unlock the Hikmalayer Wallet extension to use its quantum-ready account."
            : "No wallet set up. Open the Hikmalayer Wallet extension first."
        );
      }
      if (connected) return [account.address];
      await requestApproval({
        id: crypto.randomUUID(),
        kind: "connect",
        origin,
        address: account.address,
      });
      await setSiteConnected(origin, true);
      return [account.address];
    }

    case "hikma_getPublicKey": {
      if (!connected || !account) throw new Error("Not connected");
      return account.publicKey;
    }

    case "hikma_signMessage": {
      if (!connected || !account) throw new Error("Not connected");
      const message = params?.message;
      if (typeof message !== "string" || !message) throw new Error("message is required");
      if (message.length > 2000) throw new Error("message is too long");

      // The signing account is pinned here, not re-read after approval: the
      // user approves a specific address, and that is the address that must
      // sign even if the active account changes while the window is open.
      const index = activeAccountIndex(vault);

      // One approval, whether or not the wallet happens to be locked: a
      // locked wallet asks for the password on the same screen that shows
      // the message. Two sequential windows made the user approve something
      // before seeing what it was.
      await requestApproval({
        id: crypto.randomUUID(),
        kind: "sign",
        origin,
        address: account.address,
        label: account.label,
        message,
      });

      const signed = await signWithVault(message, index, account.scheme);
      const result = {
        signature: signed.signature,
        publicKey: account.publicKey,
        address: account.address,
      };
      // Present only for a hybrid account, and then always both: a caller
      // that sees `pqSignature` must send `pqPublicKey` with it, because the
      // node checks that the pair derives to the sending address.
      if (account.scheme === "hybrid") {
        result.pqSignature = signed.pqSignature;
        result.pqPublicKey = account.pqPublicKey;
      }
      return result;
    }

    default:
      throw new Error(`Unsupported method: ${method}`);
  }
}

// ---- Popup-facing API ---------------------------------------------------
async function handlePopupRequest(message) {
  switch (message.type) {
    // Emitted by this worker for open popups; harmless to receive back.
    case "wallet:state-changed":
      return { ok: true };

    case "wallet:state": {
      const vault = await getVault();
      const accounts = vaultAccounts(vault);
      const active = activeAccountIndex(vault);
      const scheme = await getScheme();
      const identity = activeIdentity(vault, scheme);
      return {
        hasWallet: accounts.length > 0,
        unlocked: isUnlocked(),
        // Each entry carries its quantum-ready address too, when known, so
        // the account list can show which address a site would actually see.
        accounts: accounts.map((entry, index) => ({
          ...entry,
          hybridAddress: hybridIdentities[index]?.address ?? null,
        })),
        activeIndex: active,
        scheme,
        // The address for the SELECTED scheme. Null in hybrid mode while
        // locked, because the ML-DSA key it depends on is not in memory.
        address: identity?.address ?? null,
        publicKey: identity?.publicKey ?? null,
        classicalAddress: accounts[active]?.address ?? null,
        hybridAddress: hybridIdentities[active]?.address ?? null,
        sites: Object.keys(await getSites()),
        nodeUrl: await getNodeUrl(),
      };
    }

    case "wallet:set-scheme": {
      const scheme = await setScheme(message.scheme);
      const vault = await getVault();
      // Switching scheme changes which account a connected site is talking
      // to, so it is an account change and must be announced as one.
      broadcastAccountsChanged(activeIdentity(vault, scheme)?.address ?? null);
      notifyPopup();
      return { scheme };
    }

    case "wallet:create": {
      const privateKey = generatePrivateKey();
      const vault = await encryptKeyring([privateKey], message.password);
      await setVault(vault);
      await openSession([privateKey]);
      return { address: vault.accounts[0].address, privateKey };
    }

    case "wallet:import": {
      if (!isValidPrivateKey(message.privateKey)) {
        throw new Error("That is not a valid 32-byte private key");
      }
      const vault = await encryptKeyring([message.privateKey], message.password);
      await setVault(vault);
      await openSession([normalizeHex(message.privateKey)]);
      return { address: vault.accounts[0].address };
    }

    case "wallet:unlock": {
      const vault = await getVault();
      const keys = await decryptKeyring(vault, message.password);
      await openSession(keys);
      // Upgrade a legacy single-key vault to the keyring format in place,
      // now that we hold the password and the plaintext.
      if (vault.version !== 2) {
        await setVault(await encryptKeyring(keys, message.password));
      }
      notifyPopup();
      // Unlocking makes the hybrid identity available, so the address a
      // connected site sees can change here too.
      const unlockedAddress =
        activeIdentity(await getVault(), await getScheme())?.address ?? null;
      broadcastAccountsChanged(unlockedAddress);
      return { address: unlockedAddress };
    }

    case "wallet:lock":
      lock();
      return { ok: true };

    case "wallet:add-account": {
      const vault = await getVault();
      if (vaultAccounts(vault).length === 0) throw new Error("No wallet on this device");
      const imported = message.privateKey ? normalizeHex(message.privateKey) : null;
      if (imported && !isValidPrivateKey(imported)) {
        throw new Error("That is not a valid 32-byte private key");
      }
      const added = imported || generatePrivateKey();
      const updated = await rewriteKeyring(message.password, ({ keys, accounts }) => ({
        keys: [...keys, added],
        labels: [...accounts.map((a) => a.label), message.label],
        // Switch to the account just added — that is what the user meant.
        activeIndex: keys.length,
      }));
      const account = updated.accounts[updated.activeIndex];
      return {
        address: account.address,
        // Only returned for a freshly generated key, so it can be backed up.
        privateKey: imported ? null : added,
      };
    }

    case "wallet:remove-account": {
      const vault = await getVault();
      const accounts = vaultAccounts(vault);
      if (accounts.length <= 1) {
        throw new Error("Removing the last account would remove the wallet");
      }
      const index = Number(message.index);
      if (!Number.isInteger(index) || index < 0 || index >= accounts.length) {
        throw new Error("No such account");
      }
      await rewriteKeyring(message.password, ({ keys, accounts: metadata }) => ({
        keys: keys.filter((_, i) => i !== index),
        labels: metadata.filter((_, i) => i !== index).map((a) => a.label),
        activeIndex: 0,
      }));
      return { ok: true };
    }

    case "wallet:rename-account": {
      // A label is public metadata, so this needs no password and no
      // re-encryption — the ciphertext is untouched.
      const vault = await getVault();
      const accounts = vaultAccounts(vault);
      const index = Number(message.index);
      if (!Number.isInteger(index) || index < 0 || index >= accounts.length) {
        throw new Error("No such account");
      }
      if (vault.version !== 2) throw new Error("Unlock the wallet first");
      vault.accounts[index].label = String(message.label || "").slice(0, 40) ||
        `Account ${index + 1}`;
      await setVault(vault);
      notifyPopup();
      return { ok: true };
    }

    case "wallet:select-account": {
      const vault = await getVault();
      const accounts = vaultAccounts(vault);
      const index = Number(message.index);
      if (!Number.isInteger(index) || index < 0 || index >= accounts.length) {
        throw new Error("No such account");
      }
      if (vault.version !== 2) throw new Error("Unlock the wallet first");
      vault.activeIndex = index;
      await setVault(vault);
      notifyPopup();
      // Connected sites learn the address changed, exactly as they would
      // from a wallet they already understand. Under the hybrid scheme that
      // is the `hkq…` address, not the classical one.
      const selected = activeIdentity(vault, await getScheme())?.address ?? null;
      broadcastAccountsChanged(selected);
      return { address: selected };
    }

    case "wallet:set-node": {
      const nodeUrl = normalizeNodeUrl(message.nodeUrl);
      await chrome.storage.local.set({ [NETWORK_KEY]: { nodeUrl } });
      notifyPopup();
      return { nodeUrl };
    }

    case "wallet:export": {
      const vault = await getVault();
      // Requires the password again even while unlocked.
      const keys = await decryptKeyring(vault, message.password);
      const index = Number.isInteger(message.index)
        ? message.index
        : activeAccountIndex(vault);
      if (!keys[index]) throw new Error("No such account");
      return { privateKey: keys[index] };
    }

    case "wallet:remove": {
      lock();
      await chrome.storage.local.remove([VAULT_KEY, SITES_KEY]);
      broadcastAccountsChanged(null);
      return { ok: true };
    }

    case "wallet:disconnect-site":
      await setSiteConnected(message.origin, false);
      notifyPopup();
      return { ok: true };

    case "request:get": {
      const entry = pending.get(message.id);
      if (!entry) return null;
      const { id, kind, origin, address, label, message: signMessage } = entry;
      return {
        id,
        kind,
        origin,
        address,
        label,
        message: signMessage,
        // Recomputed rather than replayed: the wallet may have been unlocked
        // since the request was queued.
        needsUnlock: !isUnlocked(),
      };
    }

    case "request:approve": {
      const entry = pending.get(message.id);
      if (!entry) throw new Error("This request has expired");
      // A locked wallet is unlocked as part of approving, on the same screen
      // that shows what is being approved.
      if (!isUnlocked()) {
        const vault = await getVault();
        await openSession(await decryptKeyring(vault, message.password));
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
// Only the request that window was showing is affected: closing some other
// window must not cancel an approval the user is still looking at.
chrome.windows.onRemoved.addListener((windowId) => {
  for (const [id, entry] of [...pending.entries()]) {
    if (entry.windowId === windowId) {
      settle(id, null, new Error("User rejected the request"));
    }
  }
});

// Expose derivation helpers for the popup's convenience (public data only).
export { deriveAddress, derivePublicKey };
