// The node client.
//
// Design rule: every write method builds the canonical message itself, from
// the same values it puts on the wire. A developer cannot accidentally sign
// one amount and submit another, because they never get to do it twice.
//
// Reads return BigInt for amounts. The API sends them as JSON numbers, which
// is exact today for every balance the chain can hold below 2^53 but not
// above it, so responses are parsed from raw text with a reviver that keeps
// large integers intact.

import { messages } from "./messages.js";
import { LocalSigner, isValidAddress } from "./signer.js";
import { applySlippage, encodeAmount, isqrt, toBaseUnits } from "./units.js";

/// Reject a malformed recipient before anything is signed.
///
/// The chain enforces this too, but catching it here gives a message that
/// names the field. There is no checksum and no recovery: a transfer to a
/// mistyped address would credit an account nobody holds a key to.
function requireAddress(value, field) {
  if (!isValidAddress(value)) {
    throw new TypeError(
      `${field} must be a native address (hkm + 40 hex characters), got ${JSON.stringify(value)}`
    );
  }
  return value;
}

/// Consensus constant: LP shares locked forever on a pool's first deposit.
export const MINIMUM_LIQUIDITY = 1_000n;

/// Fields that are amounts, and must survive JSON as exact integers.
const AMOUNT_FIELDS = new Set([
  "balance",
  "amount",
  "amount_in",
  "amount_out",
  "amount_hkm",
  "amount_token",
  "burned",
  "base_fee",
  "min_out",
  "min_hkm",
  "min_token",
  "min_shares",
  "reserve_hkm",
  "reserve_token",
  "shares",
  "total_shares",
  "stake",
  "staked",
  "total_supply",
  "initial_supply",
  "vested",
  "released",
  "releasable",
]);

/// Parse JSON, turning amount fields into BigInt without losing digits.
///
/// `JSON.parse` would coerce a 10^17 balance to a float and drop the low
/// digits before any reviver could see the original text, so the digits are
/// recovered from the source with `context.source` (Node 20+) where
/// available, and from the number otherwise.
function parseAmounts(text) {
  return JSON.parse(text, function reviver(key, value, context) {
    if (!AMOUNT_FIELDS.has(key)) return value;
    if (typeof value === "bigint") return value;
    const raw = context?.source;
    if (typeof raw === "string" && /^-?\d+$/.test(raw)) return BigInt(raw);
    if (typeof value === "number" && Number.isSafeInteger(value)) return BigInt(value);
    if (typeof value === "string" && /^-?\d+$/.test(value)) return BigInt(value);
    return value;
  });
}

export class HikmalayerError extends Error {
  constructor(message, { status, body, endpoint } = {}) {
    super(message);
    this.name = "HikmalayerError";
    this.status = status;
    this.body = body;
    this.endpoint = endpoint;
  }
}

export class HikmalayerClient {
  /// @param {object} options
  /// @param {string} options.url       node base URL
  /// @param {object} options.signer    LocalSigner / ExtensionSigner / any
  ///                                   object with `address`, `publicKey`
  ///                                   and `sign(message)`
  /// @param {string} options.adminToken  for admin-gated endpoints
  constructor({ url = "http://127.0.0.1:3000", signer, adminToken, fetch: fetchImpl } = {}) {
    this.url = String(url).replace(/\/+$/, "");
    this.signer = signer ?? null;
    this.adminToken = adminToken ?? null;
    this.fetch = fetchImpl ?? globalThis.fetch;
    if (!this.fetch) throw new Error("No fetch implementation available");
  }

  /// Attach or replace the signer. Returns `this` so it chains.
  withSigner(signer) {
    this.signer = signer;
    return this;
  }

  static withPrivateKey(privateKeyHex, options = {}) {
    return new HikmalayerClient({ ...options, signer: new LocalSigner(privateKeyHex) });
  }

  // ---------------------------------------------------------------- transport

  async #request(method, path, { body, admin = false } = {}) {
    const headers = {};
    if (body !== undefined) headers["content-type"] = "application/json";
    if (admin) {
      if (!this.adminToken) {
        throw new HikmalayerError(`${path} requires an admin token`, { endpoint: path });
      }
      headers["x-admin-token"] = this.adminToken;
    }
    const response = await this.fetch(`${this.url}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text();
    let parsed;
    try {
      parsed = text ? parseAmounts(text) : null;
    } catch {
      parsed = text;
    }
    if (!response.ok) {
      const detail = parsed?.message || response.statusText || "request failed";
      throw new HikmalayerError(`${method} ${path}: ${detail}`, {
        status: response.status,
        body: parsed,
        endpoint: path,
      });
    }
    // The node reports domain failures with 200 + {status:"error"}; surface
    // those as errors too, so callers cannot mistake one for success.
    if (parsed && typeof parsed === "object" && parsed.status === "error") {
      throw new HikmalayerError(parsed.message || "transaction rejected", {
        status: response.status,
        body: parsed,
        endpoint: path,
      });
    }
    return parsed;
  }

  get(path) {
    return this.#request("GET", path);
  }

  post(path, body, options = {}) {
    return this.#request("POST", path, { body, ...options });
  }

  // -------------------------------------------------------------------- reads

  stats() {
    return this.get("/blockchain/stats");
  }

  state() {
    return this.get("/blockchain/state");
  }

  blocks() {
    return this.get("/blocks");
  }

  block(index) {
    return this.get(`/blocks/${index}`);
  }

  validators() {
    return this.get("/staking/validators");
  }

  /// Native HKM balance, in base units.
  async balance(address = this.#requireAddress()) {
    return (await this.get(`/tokens/balance/${encodeURIComponent(address)}`)).balance;
  }

  /// The nonce the next transaction from this account must carry.
  async nextNonce(address = this.#requireAddress()) {
    return Number((await this.get(`/tokens/nonce/${encodeURIComponent(address)}`)).next_nonce);
  }

  assets() {
    return this.get("/assets");
  }

  asset(tokenId) {
    return this.get(`/assets/${encodeURIComponent(tokenId)}`);
  }

  async assetBalance(tokenId, address = this.#requireAddress()) {
    const path = `/assets/${encodeURIComponent(tokenId)}/balance/${encodeURIComponent(address)}`;
    return (await this.get(path)).balance;
  }

  pools() {
    return this.get("/dex/pools");
  }

  pool(tokenId) {
    return this.get(`/dex/pool/${encodeURIComponent(tokenId)}`);
  }

  lpPosition(tokenId, address = this.#requireAddress()) {
    return this.get(
      `/dex/position/${encodeURIComponent(tokenId)}/${encodeURIComponent(address)}`
    );
  }

  vesting(address = this.#requireAddress()) {
    return this.get(`/vesting/${encodeURIComponent(address)}`);
  }

  /// Read-only quote from the node: exactly what the on-chain math produces.
  quote(tokenId, { hkmToToken = true, amountIn }) {
    const direction = hkmToToken ? "hkm_to_token" : "token_to_hkm";
    return this.get(
      `/dex/quote/${encodeURIComponent(tokenId)}/${direction}/${encodeAmount(amountIn)}`
    );
  }

  // ------------------------------------------------------------------- writes

  #requireAddress() {
    if (!this.signer?.address) {
      throw new Error("This call needs a signer (or an explicit address)");
    }
    return this.signer.address;
  }

  async #authorize(message) {
    if (!this.signer) throw new Error("No signer configured");
    if (!this.signer.publicKey) {
      throw new Error("Signer has no public key — call connect() first");
    }
    return { public_key: this.signer.publicKey, signature: await this.signer.sign(message) };
  }

  /// Resolve the nonce once, so the message and the request cannot disagree.
  async #nonce(explicit) {
    return explicit ?? (await this.nextNonce());
  }

  async transfer({ to, amount, nonce } = {}) {
    const from = this.#requireAddress();
    requireAddress(to, "to");
    const n = await this.#nonce(nonce);
    const value = toBaseUnits(amount);
    const signed = await this.#authorize(messages.transfer({ from, to, amount: value, nonce: n }));
    return this.post("/tokens/transfer", {
      from,
      to,
      amount: encodeAmount(value),
      nonce: n,
      ...signed,
    });
  }

  async stake({ amount, vrfPublicKey, nonce } = {}) {
    const address = this.#requireAddress();
    const n = await this.#nonce(nonce);
    const value = toBaseUnits(amount);
    const signed = await this.#authorize(
      messages.stake({ address, amount: value, nonce: n, vrfPublicKey })
    );
    return this.post("/staking/deposit", {
      address,
      amount: encodeAmount(value),
      nonce: n,
      vrf_public_key: vrfPublicKey,
      ...signed,
    });
  }

  async withdrawStake({ amount, nonce } = {}) {
    const address = this.#requireAddress();
    const n = await this.#nonce(nonce);
    const value = toBaseUnits(amount);
    const signed = await this.#authorize(
      messages.withdraw({ address, amount: value, nonce: n })
    );
    return this.post("/staking/withdraw", {
      address,
      amount: encodeAmount(value),
      nonce: n,
      ...signed,
    });
  }

  async vest({ to, amount, cliffBlocks, durationBlocks, nonce } = {}) {
    const from = this.#requireAddress();
    requireAddress(to, "to");
    const n = await this.#nonce(nonce);
    const value = toBaseUnits(amount);
    const signed = await this.#authorize(
      messages.vest({ from, to, amount: value, cliffBlocks, durationBlocks, nonce: n })
    );
    return this.post("/tokens/vest", {
      from,
      to,
      amount: encodeAmount(value),
      cliff_blocks: cliffBlocks,
      duration_blocks: durationBlocks,
      nonce: n,
      ...signed,
    });
  }

  async createAsset({ symbol, name = "", decimals, initialSupply, nonce } = {}) {
    const creator = this.#requireAddress();
    const n = await this.#nonce(nonce);
    const supply = toBaseUnits(initialSupply);
    const signed = await this.#authorize(
      messages.tokenCreate({ symbol, name, decimals, initialSupply: supply, nonce: n })
    );
    return this.post("/assets/create", {
      creator,
      symbol,
      name,
      decimals,
      initial_supply: encodeAmount(supply),
      nonce: n,
      ...signed,
    });
  }

  async transferAsset({ tokenId, to, amount, nonce } = {}) {
    const from = this.#requireAddress();
    requireAddress(to, "to");
    const n = await this.#nonce(nonce);
    const value = toBaseUnits(amount);
    const signed = await this.#authorize(
      messages.tokenTransfer({ tokenId, to, amount: value, nonce: n })
    );
    return this.post("/assets/transfer", {
      token_id: tokenId,
      from,
      to,
      amount: encodeAmount(value),
      nonce: n,
      ...signed,
    });
  }

  async burnAsset({ tokenId, amount, nonce } = {}) {
    const from = this.#requireAddress();
    const n = await this.#nonce(nonce);
    const value = toBaseUnits(amount);
    const signed = await this.#authorize(
      messages.tokenBurn({ tokenId, amount: value, nonce: n })
    );
    return this.post("/assets/burn", {
      token_id: tokenId,
      from,
      amount: encodeAmount(value),
      nonce: n,
      ...signed,
    });
  }

  // ---------------------------------------------------------------------- DEX

  /// Swap against a pool.
  ///
  /// `slippage` (percent, default 0.5) sets `min_out` from a live quote. Pass
  /// `minOut` to specify the bound directly. There is no way to submit
  /// without a bound by accident: omitting both still quotes and applies the
  /// default, because an unbounded swap is a gift to whoever orders it.
  async swap({ tokenId, hkmToToken = true, amountIn, minOut, slippage = 0.5, nonce } = {}) {
    const trader = this.#requireAddress();
    const value = toBaseUnits(amountIn);
    let bound;
    if (minOut !== undefined) {
      bound = toBaseUnits(minOut);
    } else {
      const quote = await this.quote(tokenId, { hkmToToken, amountIn: value });
      if (!quote?.amount_out) throw new Error(`No liquidity to quote ${tokenId}`);
      bound = applySlippage(quote.amount_out, slippage);
    }
    const n = await this.#nonce(nonce);
    const signed = await this.#authorize(
      messages.ammSwap({ tokenId, hkmToToken, amountIn: value, minOut: bound, nonce: n })
    );
    return this.post("/dex/swap", {
      token_id: tokenId,
      trader,
      hkm_to_token: hkmToToken,
      amount_in: encodeAmount(value),
      min_out: encodeAmount(bound),
      nonce: n,
      ...signed,
    });
  }

  /// Predict an AddLiquidity outcome — mirrors `ChainState::apply` exactly.
  /// Returns the amounts the chain will actually take (the pool ratio binds
  /// on one side) and the LP shares minted, or `null` if it cannot mint.
  static quoteAddLiquidity(pool, amountHkm, amountToken) {
    const offeredHkm = toBaseUnits(amountHkm);
    const offeredToken = toBaseUnits(amountToken);
    if (offeredHkm <= 0n || offeredToken <= 0n) return null;

    const rh = toBaseUnits(pool?.reserve_hkm ?? 0);
    const rt = toBaseUnits(pool?.reserve_token ?? 0);
    const total = toBaseUnits(pool?.total_shares ?? 0);

    if (rh === 0n || rt === 0n || total === 0n) {
      const shares = isqrt(offeredHkm * offeredToken);
      if (shares <= MINIMUM_LIQUIDITY) return null;
      return {
        first: true,
        useHkm: offeredHkm,
        useToken: offeredToken,
        minted: shares - MINIMUM_LIQUIDITY,
      };
    }

    const tokenOptimal = (offeredHkm * rt) / rh;
    const [useHkm, useToken] =
      tokenOptimal <= offeredToken
        ? [offeredHkm, tokenOptimal]
        : [(offeredToken * rh) / rt, offeredToken];
    if (useHkm === 0n || useToken === 0n) return null;

    const fromHkm = (useHkm * total) / rh;
    const fromToken = (useToken * total) / rt;
    const minted = fromHkm < fromToken ? fromHkm : fromToken;
    return minted === 0n ? null : { first: false, useHkm, useToken, minted };
  }

  /// Predict a RemoveLiquidity outcome: the pro-rata share of both reserves.
  static quoteRemoveLiquidity(pool, shares) {
    const burn = toBaseUnits(shares);
    const rh = toBaseUnits(pool?.reserve_hkm ?? 0);
    const rt = toBaseUnits(pool?.reserve_token ?? 0);
    const total = toBaseUnits(pool?.total_shares ?? 0);
    if (burn <= 0n || total <= 0n || rh <= 0n || rt <= 0n) return null;
    const amountHkm = (burn * rh) / total;
    const amountToken = (burn * rt) / total;
    if (amountHkm === 0n || amountToken === 0n) return null;
    return { amountHkm, amountToken };
  }

  /// Deposit into a pool. Bounds default to a `slippage` percent below the
  /// predicted shares; a bound is never omitted.
  async addLiquidity({
    tokenId,
    amountHkm,
    amountToken,
    minShares,
    slippage = 0.5,
    nonce,
  } = {}) {
    const provider = this.#requireAddress();
    const hkm = toBaseUnits(amountHkm);
    const token = toBaseUnits(amountToken);
    let bound;
    if (minShares !== undefined) {
      bound = toBaseUnits(minShares);
    } else {
      const pool = await this.pool(tokenId).catch(() => null);
      const prediction = HikmalayerClient.quoteAddLiquidity(pool, hkm, token);
      if (!prediction) throw new Error("These amounts would mint no LP shares");
      bound = applySlippage(prediction.minted, slippage);
    }
    if (bound < 1n) bound = 1n; // a bound of zero is no bound
    const n = await this.#nonce(nonce);
    const signed = await this.#authorize(
      messages.ammAdd({
        tokenId,
        amountHkm: hkm,
        amountToken: token,
        minShares: bound,
        nonce: n,
      })
    );
    return this.post("/dex/add", {
      token_id: tokenId,
      provider,
      amount_hkm: encodeAmount(hkm),
      amount_token: encodeAmount(token),
      min_shares: encodeAmount(bound),
      nonce: n,
      ...signed,
    });
  }

  /// Burn LP shares and withdraw the underlying assets.
  async removeLiquidity({
    tokenId,
    shares,
    minHkm,
    minToken,
    slippage = 0.5,
    nonce,
  } = {}) {
    const provider = this.#requireAddress();
    const burn = toBaseUnits(shares);
    let boundHkm;
    let boundToken;
    if (minHkm !== undefined && minToken !== undefined) {
      boundHkm = toBaseUnits(minHkm);
      boundToken = toBaseUnits(minToken);
    } else {
      const pool = await this.pool(tokenId);
      const prediction = HikmalayerClient.quoteRemoveLiquidity(pool, burn);
      if (!prediction) throw new Error("These shares would withdraw nothing");
      boundHkm = applySlippage(prediction.amountHkm, slippage);
      boundToken = applySlippage(prediction.amountToken, slippage);
    }
    if (boundHkm < 1n) boundHkm = 1n;
    if (boundToken < 1n) boundToken = 1n;
    const n = await this.#nonce(nonce);
    const signed = await this.#authorize(
      messages.ammRemove({
        tokenId,
        shares: burn,
        minHkm: boundHkm,
        minToken: boundToken,
        nonce: n,
      })
    );
    return this.post("/dex/remove", {
      token_id: tokenId,
      provider,
      shares: encodeAmount(burn),
      min_hkm: encodeAmount(boundHkm),
      min_token: encodeAmount(boundToken),
      nonce: n,
      ...signed,
    });
  }

  // ------------------------------------------------------------------- admin

  /// Mine a block. Admin-gated; on a devnet this is how you make queued
  /// transactions take effect.
  mine() {
    return this.post("/mine", undefined, { admin: true });
  }

  /// Send HKM from the node's treasury. Devnet convenience; a node only
  /// enables it when it holds the treasury key.
  faucet({ to, amount }) {
    requireAddress(to, "to");
    return this.post("/tokens/faucet", { to, amount: encodeAmount(amount) }, { admin: true });
  }

  /// Wait until a predicate over `stats()` holds. Useful after submitting a
  /// transaction, since submission only queues it.
  async waitFor(predicate, { timeoutMs = 30_000, intervalMs = 500 } = {}) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const stats = await this.stats();
      if (await predicate(stats)) return stats;
      if (Date.now() > deadline) throw new Error("Timed out waiting for the chain");
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
  }
}
