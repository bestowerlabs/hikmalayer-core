// End-to-end: drive a live devnet with nothing but the SDK.
//
// Needs a running node with a treasury key (so the faucet works) and an admin
// token. `ops/devnet.sh` provides exactly that:
//
//   ops/devnet.sh &
//   HIKMALAYER_ADMIN_TOKEN=devadmin npm run test:integration
//
// Skips itself when no node is reachable, so `npm test` stays offline-safe.

import { test, describe, before } from "node:test";
import assert from "node:assert/strict";

import {
  HikmalayerClient,
  LocalSigner,
  formatUnits,
  parseUnits,
} from "../../src/index.js";

const URL = process.env.HIKMALAYER_NODE ?? "http://127.0.0.1:3000";
const ADMIN = process.env.HIKMALAYER_ADMIN_TOKEN ?? "devadmin";

const reachable = await fetch(`${URL}/blockchain/stats`)
  .then((r) => r.ok)
  .catch(() => false);

describe("live chain", { skip: reachable ? false : `no node at ${URL}` }, () => {
  let alice;
  let bob;
  let admin;

  /// Queued transactions only take effect once mined — and a transaction is
  /// not guaranteed to make the very next block. Sealing one block and then
  /// asserting races the chain: whichever test loses reads pre-transaction
  /// state and fails, on a different test each run.
  ///
  /// Waiting for the mempool to drain is the general form of "my transaction
  /// landed". This suite is sequential, so nothing else is filling it.
  const settle = async () => {
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline) {
      // Not this slot's leader is fine — the devnet timer will seal it.
      await admin.mine().catch(() => {});
      await new Promise((r) => setTimeout(r, 300));
      const { pending_transactions } = await admin.stats();
      if (pending_transactions === 0) return;
    }
    throw new Error("Timed out after 60s waiting for the mempool to drain");
  };

  before(async () => {
    admin = new HikmalayerClient({ url: URL, adminToken: ADMIN });
    alice = new HikmalayerClient({ url: URL, adminToken: ADMIN, signer: LocalSigner.random() });
    bob = new HikmalayerClient({ url: URL, signer: LocalSigner.random() });

    await admin.faucet({ to: alice.signer.address, amount: parseUnits("100000") });
    await settle();
  });

  test("reads chain state with exact BigInt amounts", async () => {
    const state = await admin.state();
    assert.equal(typeof state.total_supply, "bigint");
    // At least the 30B HKM premine (block rewards push it higher). The point
    // is that it is beyond 2^53 and still exact.
    assert.ok(state.total_supply >= 30_000_000_000_000_000n, `${state.total_supply}`);
    assert.ok(state.total_supply > BigInt(Number.MAX_SAFE_INTEGER));
  });

  test("faucet funded the account", async () => {
    assert.equal(await alice.balance(), parseUnits("100000"));
  });

  test("transfers native HKM", async () => {
    await alice.transfer({ to: bob.signer.address, amount: parseUnits("250.5") });
    await settle();
    assert.equal(await bob.balance(), parseUnits("250.5"));
    assert.equal(formatUnits(await bob.balance()), "250.5");
  });

  test("nonces advance without being managed by hand", async () => {
    const before = await alice.nextNonce();
    await alice.transfer({ to: bob.signer.address, amount: parseUnits("1") });
    await settle();
    assert.equal(await alice.nextNonce(), before + 1);
  });

  test("rejects a transfer the sender cannot afford", async () => {
    await assert.rejects(
      () => bob.transfer({ to: alice.signer.address, amount: parseUnits("1000000000") }),
      /Insufficient balance|not applicable/i
    );
  });

  let tokenId;

  test("issues a native token", async () => {
    const res = await alice.createAsset({
      symbol: "SDKT",
      name: "SDK Test Token",
      decimals: 6,
      initialSupply: parseUnits("1000000"),
    });
    tokenId = res.message.match(/token_id=(\S+)/)?.[1];
    assert.ok(tokenId?.startsWith("hkt"), res.message);
    await settle();

    const asset = await alice.asset(tokenId);
    assert.equal(asset.symbol, "SDKT");
    assert.equal(await alice.assetBalance(tokenId), parseUnits("1000000"));
  });

  test("transfers token units", async () => {
    await alice.transferAsset({
      tokenId,
      to: bob.signer.address,
      amount: parseUnits("1000"),
    });
    await settle();
    assert.equal(await alice.assetBalance(tokenId, bob.signer.address), parseUnits("1000"));
  });

  test("seeds a pool, and the predicted shares are what the chain mints", async () => {
    const amountHkm = parseUnits("10000");
    const amountToken = parseUnits("50000");
    const predicted = HikmalayerClient.quoteAddLiquidity(null, amountHkm, amountToken);

    await alice.addLiquidity({ tokenId, amountHkm, amountToken });
    await settle();

    const position = await alice.lpPosition(tokenId);
    assert.equal(position.shares, predicted.minted, "SDK prediction diverged from consensus");

    const pool = await alice.pool(tokenId);
    assert.equal(pool.reserve_hkm, amountHkm);
    assert.equal(pool.reserve_token, amountToken);
  });

  test("swaps, with a slippage bound applied by default", async () => {
    const amountIn = parseUnits("100");
    const quote = await bob.quote(tokenId, { hkmToToken: true, amountIn });
    assert.ok(quote.amount_out > 0n);

    const before = await bob.assetBalance(tokenId);
    await bob.swap({ tokenId, hkmToToken: true, amountIn });
    await settle();

    const gained = (await bob.assetBalance(tokenId)) - before;
    assert.equal(gained, quote.amount_out, "swap output differed from the quote");

    const pool = await bob.pool(tokenId);
    assert.equal(pool.reserve_hkm, parseUnits("10000") + amountIn);
  });

  test("an unsatisfiable slippage bound is refused by the chain", async () => {
    await assert.rejects(
      () =>
        bob.swap({
          tokenId,
          hkmToToken: true,
          amountIn: parseUnits("10"),
          minOut: parseUnits("999999999"),
        }),
      /slippage|not applicable/i
    );
  });

  test("withdraws liquidity, matching the predicted amounts", async () => {
    const pool = await alice.pool(tokenId);
    const shares = (await alice.lpPosition(tokenId)).shares / 2n;
    const predicted = HikmalayerClient.quoteRemoveLiquidity(pool, shares);

    const hkmBefore = await alice.balance();
    await alice.removeLiquidity({ tokenId, shares });
    await settle();

    const after = await alice.pool(tokenId);
    assert.equal(pool.reserve_hkm - after.reserve_hkm, predicted.amountHkm);
    assert.equal(pool.reserve_token - after.reserve_token, predicted.amountToken);
    // Received the HKM, less the transaction fee.
    assert.ok((await alice.balance()) > hkmBefore);
  });

  test("burning token units reduces supply", async () => {
    const before = (await alice.asset(tokenId)).total_supply;
    await alice.burnAsset({ tokenId, amount: parseUnits("100") });
    await settle();
    assert.equal((await alice.asset(tokenId)).total_supply, before - parseUnits("100"));
  });

  test("an amount past 2^53 survives the whole round trip", async () => {
    // Beyond Number.MAX_SAFE_INTEGER: the case that silently corrupted before.
    const huge = 9_007_199_254_740_993n;
    const carol = new HikmalayerClient({ url: URL, signer: LocalSigner.random() });
    await admin.faucet({ to: carol.signer.address, amount: huge });
    await settle();
    assert.equal(await carol.balance(), huge, "amount changed in transit");
  });

  test("refuses a malformed recipient before signing anything", async () => {
    // There is no checksum and no recovery: crediting a mistyped address
    // would put the funds where nobody holds a key.
    await assert.rejects(
      () => bob.transfer({ to: "not-an-address", amount: 1n }),
      /must be a native address/
    );
    // And the chain refuses it too, even if a client skipped the guard.
    // Signed properly (scoped to this network) so the request reaches the
    // recipient check rather than failing earlier on the signature.
    const nonce = await bob.nextNonce();
    const chainId = await bob.resolveChainId();
    const signed = await bob.signer.sign(
      `${chainId}:hikmalayer-transfer:${bob.signer.address}:typo-address:1:${nonce}`
    );
    await assert.rejects(
      () =>
        bob.post("/tokens/transfer", {
          from: bob.signer.address,
          to: "typo-address",
          amount: "1",
          nonce,
          public_key: bob.signer.publicKey,
          signature: signed,
        }),
      /not a valid hkm address/
    );
  });

  test("a lookup that finds nothing returns null rather than throwing", async () => {
    // The node answers 200 + null for an unknown asset; that is an answer,
    // not a failure, and the SDK passes it through unchanged.
    assert.equal(await bob.asset("hktdoesnotexist"), null);
  });

  test("surfaces node-side rejections as errors, not silent successes", async () => {
    // The node reports domain failures with HTTP 200 and {status:"error"},
    // which a naive client would read as success.
    await assert.rejects(
      () =>
        bob.swap({
          tokenId: "hktdoesnotexist",
          amountIn: 1_000n,
          minOut: 1n,
        }),
      (error) => error.name === "HikmalayerError" && /No pool|not applicable/i.test(error.message)
    );
  });
});
