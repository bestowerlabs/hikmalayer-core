import { safeText } from "../../dashboard/src/lib/sanitize.js";
// Popup and approval UI.
//
// This page runs in the extension's own context under the extension CSP. A
// website cannot render into it, read it, or click its buttons — which is the
// whole point: approvals happen somewhere the page cannot reach.

const app = document.getElementById("app");
const params = new URLSearchParams(location.search);
const requestId = params.get("request");

const send = async (message) => {
  const response = await chrome.runtime.sendMessage(message);
  if (!response?.ok) throw new Error(response?.error || "Wallet error");
  return response.result;
};

const el = (html) => {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild;
};

const escape = (value) =>
  String(value ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])
  );

/// Untrusted strings (origins, signing messages) are stripped of invisible
/// and text-reordering characters before display, using the same tested
/// helper the dashboard uses — nothing can be hidden from the approver.
const safe = (value, max = 400) => safeText(value, max);

function render(node) {
  app.replaceChildren(node);
}

function showError(container, message) {
  const existing = container.querySelector(".err");
  if (existing) existing.remove();
  container.appendChild(el(`<p class="err">${escape(message)}</p>`));
}

// ===== Approval views (opened with ?request=<id>) =====

async function renderApproval() {
  let request;
  try {
    request = await send({ type: "request:get", id: requestId });
  } catch {
    request = null;
  }
  if (!request) {
    render(el(`<div><h1>Hikmalayer Wallet</h1><p class="muted">This request has expired.</p></div>`));
    return;
  }

  const origin = safe(request.origin, 80);

  if (request.kind === "connect") {
    const view = el(`
      <div>
        <h1>Connect site</h1>
        <p class="muted">A website is asking to see your address.</p>
        <div class="card">
          <p class="muted" style="margin:0 0 6px">Site</p>
          <div class="mono" style="font-size:12px;color:#fff">${escape(origin)}</div>
        </div>
        <div class="card">
          <p class="muted" style="margin:0 0 6px">Account</p>
          <div class="addr">${escape(request.address)}</div>
        </div>
        <p class="muted">Connecting lets this site read your address and ask you to sign.
        It can never move funds on its own — every signature needs your approval.</p>
        <div class="row">
          <button class="ghost" id="reject">Reject</button>
          <button id="approve">Connect</button>
        </div>
      </div>`);
    view.querySelector("#approve").onclick = async () => {
      await send({ type: "request:approve", id: requestId });
      window.close();
    };
    view.querySelector("#reject").onclick = async () => {
      await send({ type: "request:reject", id: requestId });
      window.close();
    };
    render(view);
    return;
  }

  // kind === "sign". A locked wallet is unlocked on this same screen: the
  // user must be able to read what they are authorizing before typing a
  // password, not after.
  const shown = safe(request.message, 600);
  const altered = shown !== String(request.message).trim();
  const view = el(`
    <div>
      <h1>Signature request</h1>
      <p class="muted">${escape(origin)} wants you to sign.</p>
      <div class="card">
        <p class="muted" style="margin:0 0 6px">Exact message to be signed</p>
        <div class="msg">${escape(shown)}</div>
      </div>
      ${
        altered
          ? `<div class="warn">⚠️ Hidden or text-reordering characters were removed for display. Do not approve unless you understand this request.</div>`
          : ""
      }
      <div class="card">
        <p class="muted" style="margin:0 0 6px">Signing account${
          request.label ? ` · ${escape(safe(request.label, 40))}` : ""
        }</p>
        <div class="addr">${escape(request.address)}</div>
      </div>
      ${
        request.needsUnlock
          ? `<div class="card">
               <p class="muted" style="margin:0 0 6px">Your wallet is locked</p>
               <input type="password" id="password" placeholder="Password" autofocus />
             </div>`
          : ""
      }
      <p class="muted">Approve only if this matches what you just did on the site.</p>
      <div class="row">
        <button class="ghost" id="reject">Reject</button>
        <button id="approve">Approve &amp; sign</button>
      </div>
    </div>`);
  const approve = async () => {
    try {
      await send({
        type: "request:approve",
        id: requestId,
        password: view.querySelector("#password")?.value,
      });
      window.close();
    } catch (error) {
      showError(view, error.message);
    }
  };
  view.querySelector("#approve").onclick = approve;
  view.querySelector("#reject").onclick = async () => {
    await send({ type: "request:reject", id: requestId });
    window.close();
  };
  render(view);
  view.querySelector("#password")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") approve();
  });
}

// ===== Main popup =====

async function renderMain() {
  const state = await send({ type: "wallet:state" });

  if (!state.hasWallet) return renderOnboarding();
  if (!state.unlocked) return renderUnlock(state);
  return renderAccount(state);
}

function renderOnboarding() {
  const view = el(`
    <div>
      <h1>Hikmalayer Wallet</h1>
      <p class="muted">Your keys stay in this extension — websites can request signatures, never read them.</p>
      <div class="tabs">
        <button class="active" data-tab="create">Create</button>
        <button data-tab="import">Import</button>
      </div>
      <div class="card">
        <input type="password" id="key" class="hidden" placeholder="Private key (64 hex chars)" autocomplete="off" />
        <input type="password" id="password" placeholder="New password (min 8 characters)" autocomplete="new-password" />
        <input type="password" id="confirm" placeholder="Confirm password" autocomplete="new-password" />
        <button id="go">Create wallet</button>
      </div>
      <p class="muted">Encrypted with AES-256-GCM; password stretched with PBKDF2-SHA256 (310,000 iterations).</p>
    </div>`);

  let tab = "create";
  view.querySelectorAll(".tabs button").forEach((button) => {
    button.onclick = () => {
      tab = button.dataset.tab;
      view.querySelectorAll(".tabs button").forEach((b) => b.classList.remove("active"));
      button.classList.add("active");
      view.querySelector("#key").classList.toggle("hidden", tab !== "import");
      view.querySelector("#go").textContent = tab === "create" ? "Create wallet" : "Import wallet";
    };
  });

  view.querySelector("#go").onclick = async () => {
    const password = view.querySelector("#password").value;
    const confirm = view.querySelector("#confirm").value;
    if (password !== confirm) return showError(view, "Passwords do not match");
    try {
      if (tab === "create") {
        const result = await send({ type: "wallet:create", password });
        renderBackup(result.privateKey);
      } else {
        await send({
          type: "wallet:import",
          password,
          privateKey: view.querySelector("#key").value.trim(),
        });
        renderMain();
      }
    } catch (error) {
      showError(view, error.message);
    }
  };
  render(view);
}

function renderBackup(privateKey) {
  const view = el(`
    <div>
      <h1>Back up your key</h1>
      <p class="muted">This is shown once. Without it, a forgotten password means a lost wallet.</p>
      <div class="warn">
        <div class="mono" style="word-break:break-all;font-size:11px">${escape(privateKey)}</div>
      </div>
      <button id="done" style="margin-top:12px">I have saved it</button>
    </div>`);
  view.querySelector("#done").onclick = () => renderMain();
  render(view);
}

function renderUnlock(state) {
  const view = el(`
    <div>
      <h1>Hikmalayer Wallet <span class="pill">Locked</span></h1>
      <p class="muted">${escape(state.classicalAddress ?? state.address ?? "")}</p>
      <div class="card">
        <input type="password" id="password" placeholder="Password" autofocus />
        <button id="unlock">Unlock</button>
      </div>
    </div>`);
  const submit = async () => {
    try {
      await send({ type: "wallet:unlock", password: view.querySelector("#password").value });
      renderMain();
    } catch (error) {
      showError(view, error.message);
    }
  };
  view.querySelector("#unlock").onclick = submit;
  view.querySelector("#password").addEventListener("keydown", (e) => {
    if (e.key === "Enter") submit();
  });
  render(view);
}

function renderAccount(state) {
  const view = el(`
    <div>
      <h1>Hikmalayer Wallet <span class="pill on">Unlocked</span></h1>
      <p class="muted">Auto-locks after 15 minutes idle.</p>
      <div class="card">
        <p class="muted" style="margin:0 0 6px">Active address</p>
        <div class="addr" id="address">${escape(state.address ?? "")}</div>
        <p class="muted" style="margin:8px 0 0">Balance: <span id="balance">…</span></p>
        <button class="ghost" id="copy" style="margin-top:8px">Copy address</button>

        <!-- One key, two accounts with separate balances. Classical is
             ECDSA; quantum-ready adds an ML-DSA-65 signature, so an attacker
             must break both schemes to spend. -->
        <div class="tabs" style="margin:12px 0 0">
          <button data-scheme="classical" class="${
            state.scheme === "hybrid" ? "" : "active"
          }">Classical</button>
          <button data-scheme="hybrid" class="${
            state.scheme === "hybrid" ? "active" : ""
          }">Quantum-ready</button>
        </div>
        <p class="tiny muted" style="margin:6px 0 0">${
          state.scheme === "hybrid"
            ? "Signs with ECDSA and ML-DSA-65. Separate account from the classical one."
            : "Signs with ECDSA only. Quantum-ready is a different account."
        }</p>
      </div>

      <div class="card">
        <h2>Accounts</h2>
        <div id="accounts"></div>
        <div class="tabs" style="margin:10px 0 8px">
          <button class="active" data-tab="new">New</button>
          <button data-tab="import">Import key</button>
        </div>
        <input type="password" id="newKey" class="hidden" placeholder="Private key (64 hex chars)" autocomplete="off" />
        <input type="text" id="newLabel" placeholder="Label (optional)" autocomplete="off" />
        <input type="password" id="addPassword" placeholder="Your password" autocomplete="current-password" />
        <button id="addAccount">Add account</button>
        <p class="muted" style="margin:8px 0 0">Adding an account re-encrypts the
        keyring, so your password is required.</p>
        <div id="added"></div>
      </div>

      <div class="card">
        <h2>Network</h2>
        <p class="muted" style="margin:0 0 8px">Node used for balances. Signing
        never contacts a node.</p>
        <input type="text" id="nodeUrl" placeholder="https://node.example" value="${escape(
          state.nodeUrl || ""
        )}" />
        <button class="ghost" id="saveNode">Save node URL</button>
        <div id="nodeStatus"></div>
      </div>

      <div class="card">
        <h2>Connected sites</h2>
        <div id="sites"></div>
      </div>

      <div class="row">
        <button class="ghost" id="lock">Lock</button>
        <button class="danger" id="remove">Remove</button>
      </div>
      <div class="card" style="margin-top:12px">
        <h2>Export private key</h2>
        <p class="muted" style="margin:0 0 8px">Exports the active account only.</p>
        <input type="password" id="exportPassword" placeholder="Confirm password" autocomplete="off" />
        <button class="danger" id="export">Reveal</button>
        <div id="revealed"></div>
      </div>
    </div>`);

  // ---- Accounts ----
  view.querySelectorAll("[data-scheme]").forEach((button) => {
    button.onclick = async () => {
      try {
        await send({ type: "wallet:set-scheme", scheme: button.dataset.scheme });
        renderMain();
      } catch (error) {
        showError(view, error.message);
      }
    };
  });

  const accounts = view.querySelector("#accounts");
  state.accounts.forEach((account, index) => {
    const isActive = index === state.activeIndex;
    const row = el(`
      <div class="acct ${isActive ? "active" : ""}">
        <span class="check">${isActive ? "●" : "○"}</span>
        <span class="who">
          <span class="name">${escape(safe(account.label, 40))}</span><br />
          <span class="tiny">${escape(account.address.slice(0, 14))}…${escape(
            account.address.slice(-6)
          )}</span>
        </span>
        <button class="ghost" data-act="rename">Rename</button>
        ${
          state.accounts.length > 1
            ? `<button class="danger" data-act="remove">✕</button>`
            : ""
        }
      </div>`);

    row.querySelector(".who").onclick = async () => {
      if (isActive) return;
      await send({ type: "wallet:select-account", index });
      renderMain();
    };
    row.querySelector('[data-act="rename"]').onclick = async (event) => {
      event.stopPropagation();
      const label = prompt("Name this account", account.label);
      if (label === null) return;
      await send({ type: "wallet:rename-account", index, label });
      renderMain();
    };
    row.querySelector('[data-act="remove"]')?.addEventListener("click", async (event) => {
      event.stopPropagation();
      if (!confirm(`Remove ${account.label}? Back up its key first — this cannot be undone.`)) {
        return;
      }
      const password = prompt("Confirm your password to re-encrypt the keyring");
      if (!password) return;
      try {
        await send({ type: "wallet:remove-account", index, password });
        renderMain();
      } catch (error) {
        showError(view, error.message);
      }
    });

    accounts.appendChild(row);
  });

  let addTab = "new";
  view.querySelectorAll(".tabs button").forEach((button) => {
    button.onclick = () => {
      addTab = button.dataset.tab;
      view.querySelectorAll(".tabs button").forEach((b) => b.classList.remove("active"));
      button.classList.add("active");
      view.querySelector("#newKey").classList.toggle("hidden", addTab !== "import");
    };
  });

  view.querySelector("#addAccount").onclick = async () => {
    const password = view.querySelector("#addPassword").value;
    const label = view.querySelector("#newLabel").value.trim();
    const privateKey =
      addTab === "import" ? view.querySelector("#newKey").value.trim() : null;
    try {
      const result = await send({ type: "wallet:add-account", password, label, privateKey });
      // A generated key is shown once, here, so it can be backed up.
      if (result.privateKey) {
        view.querySelector("#added").replaceChildren(
          el(`<div class="warn" style="margin-top:8px">
                <p style="margin:0 0 6px">Back up this key — it is shown once.</p>
                <div class="mono" style="word-break:break-all;font-size:11px">${escape(
                  result.privateKey
                )}</div>
              </div>`)
        );
      } else {
        renderMain();
      }
      view.querySelector("#addPassword").value = "";
      view.querySelector("#newKey").value = "";
      view.querySelector("#newLabel").value = "";
    } catch (error) {
      showError(view, error.message);
    }
  };

  // ---- Network ----
  view.querySelector("#saveNode").onclick = async () => {
    try {
      const { nodeUrl } = await send({
        type: "wallet:set-node",
        nodeUrl: view.querySelector("#nodeUrl").value,
      });
      view
        .querySelector("#nodeStatus")
        .replaceChildren(el(`<p class="ok">Saved: ${escape(nodeUrl)}</p>`));
    } catch (error) {
      showError(view, error.message);
    }
  };

  const sites = view.querySelector("#sites");
  if (state.sites.length === 0) {
    sites.appendChild(el(`<p class="muted" style="margin:0">No sites connected.</p>`));
  } else {
    state.sites.forEach((origin) => {
      const row = el(
        `<div class="site"><span class="mono">${escape(safe(origin, 44))}</span>
         <button class="ghost">Disconnect</button></div>`
      );
      row.querySelector("button").onclick = async () => {
        await send({ type: "wallet:disconnect-site", origin });
        renderMain();
      };
      sites.appendChild(row);
    });
  }

  view.querySelector("#copy").onclick = () => navigator.clipboard.writeText(state.address);
  view.querySelector("#lock").onclick = async () => {
    await send({ type: "wallet:lock" });
    renderMain();
  };
  view.querySelector("#remove").onclick = async () => {
    if (confirm("Remove this wallet? Without your key backup it cannot be recovered.")) {
      await send({ type: "wallet:remove" });
      renderMain();
    }
  };
  view.querySelector("#export").onclick = async () => {
    try {
      const { privateKey } = await send({
        type: "wallet:export",
        password: view.querySelector("#exportPassword").value,
      });
      view.querySelector("#revealed").replaceChildren(
        el(`<div class="warn" style="margin-top:8px"><div class="mono"
             style="word-break:break-all;font-size:11px">${escape(privateKey)}</div></div>`)
      );
      view.querySelector("#exportPassword").value = "";
    } catch (error) {
      showError(view, error.message);
    }
  };

  render(view);

  // Balance is a nicety: failure to reach a node must not break the popup.
  const nodeUrl = (state.nodeUrl || "http://127.0.0.1:3000").replace(/\/+$/, "");
  fetch(`${nodeUrl}/tokens/balance/${encodeURIComponent(state.address)}`)
    .then((r) => r.json())
    .then((data) => {
      const hkm = (Number(data.balance ?? 0) / 1_000_000).toLocaleString(undefined, {
        maximumFractionDigits: 6,
      });
      view.querySelector("#balance").textContent = `${hkm} HKM`;
    })
    .catch(() => {
      view.querySelector("#balance").textContent = "node unreachable";
    });
}

(requestId ? renderApproval() : renderMain()).catch((error) => {
  render(el(`<div><h1>Hikmalayer Wallet</h1><p class="err">${escape(error.message)}</p></div>`));
});
