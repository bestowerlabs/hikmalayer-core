// End-to-end: a quantum-ready (`hkq…`) account against a live node.
//
// The offline tests prove the cryptography matches the Rust. This proves the
// chain actually accepts it: a hybrid account funded, spending, issuing a
// token, and trading on the native AMM — and refusing a transaction that
// drops the post-quantum half.
//
//   ops/devnet.sh
//   HIKMALAYER_ADMIN_TOKEN=devadmin npm run test:integration
//
// Skips itself when no node is reachable.

import { test, describe, before } from "node:test";
import assert from "node:assert/strict";

import { HikmalayerClient, HybridSigner, generatePrivateKey, parseUnits } from "../../src/index.js";

const URL = process.env.HIKMALAYER_NODE ?? "http://127.0.0.1:3000";
const ADMIN = process.env.HIKMALAYER_ADMIN_TOKEN ?? "devadmin";

async function nodeIsUp() {
  try {
    const response = await fetch(`${URL}/blockchain/stats`);
    return response.ok;
  } catch {
    return false;
  }
}

const UP = await nodeIsUp();

/// Mine until `check` holds.
///
/// Test files run in parallel and the devnet also seals on a timer, so a
/// single `mine()` can lose the race for a block — the transaction is queued
/// either way, it just lands one block later. Polling keeps that from looking
/// like a rejection.
async function settle(client, check, attempts = 8) {
  for (let i = 0; i < attempts; i += 1) {
    await client.mine().catch(() => {});
    const result = await check();
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  return check();
}

describe("hybrid account, live chain", { skip: UP ? false : `no node at ${URL}` }, () => {
  let client;
  let signer;

  before(async () => {
    signer = new HybridSigner(generatePrivateKey());
    client = new HikmalayerClient({ url: URL, signer, adminToken: ADMIN });
    assert.ok(signer.address.startsWith("hkq"), "signer should control a hybrid account");
  });

  test("the node funds and credits a hybrid address", async () => {
    await client.faucet({ to: signer.address, amount: parseUnits("500") });
    const balance = await settle(client, async () => {
      const value = await client.balance();
      return value === parseUnits("500") ? value : null;
    });
    assert.equal(balance, parseUnits("500"), `funded balance was ${balance}`);
  });

  test("a dual-signed transfer is accepted", async () => {
    const payee = new HybridSigner(generatePrivateKey()).address;
    await client.transfer({ to: payee, amount: parseUnits("25") });
    const received = await settle(client, async () => {
      const value = await client.balance(payee);
      return value === parseUnits("25") ? value : null;
    });
    assert.equal(received, parseUnits("25"));
  });

  test("the same transfer without the post-quantum half is refused", async () => {
    const payee = new HybridSigner(generatePrivateKey()).address;
    const nonce = await client.nextNonce();
    const amount = parseUnits("10");
    const chainId = await client.resolveChainId();
    const message =
      `${chainId}:hikmalayer-transfer:${signer.address}:${payee}:${amount}:${nonce}`;
    const both = await signer.sign(message);

    // Everything the attacker with a broken secp256k1 would have: a genuine
    // ECDSA signature over the exact message, and nothing else.
    const response = await fetch(`${URL}/tokens/transfer`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        from: signer.address,
        to: payee,
        amount: amount.toString(),
        nonce,
        public_key: signer.publicKey,
        signature: both.signature,
      }),
    });
    const body = await response.json();
    assert.equal(body.status, "error", `node accepted a downgraded transaction: ${body.message}`);
    assert.match(body.message, /post-quantum/i);

    await client.mine();
    assert.equal(await client.balance(payee), 0n, "the refused transfer moved funds anyway");
  });

  test("a substituted post-quantum key is refused", async () => {
    const attacker = new HybridSigner(generatePrivateKey());
    const payee = attacker.address;
    const nonce = await client.nextNonce();
    const amount = parseUnits("10");
    const chainId = await client.resolveChainId();
    const message =
      `${chainId}:hikmalayer-transfer:${signer.address}:${payee}:${amount}:${nonce}`;

    const victimHalf = await signer.sign(message);
    const attackerHalf = await attacker.sign(message);

    const response = await fetch(`${URL}/tokens/transfer`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        from: signer.address,
        to: payee,
        amount: amount.toString(),
        nonce,
        // Victim's classical key (the broken one) + attacker's ML-DSA key.
        public_key: signer.publicKey,
        signature: victimHalf.signature,
        pq_public_key: attacker.pqPublicKey,
        pq_signature: attackerHalf.pqSignature,
      }),
    });
    const body = await response.json();
    assert.equal(body.status, "error", `node accepted a substituted key: ${body.message}`);
    await client.mine();
    assert.equal(await client.balance(payee), 0n);
  });

  test("a hybrid account issues a native token and trades it on the native AMM", async () => {
    const symbol = `HQ${Math.floor(Math.random() * 9000 + 1000)}`;
    const created = await client.createAsset({
      symbol,
      name: "Quantum-ready test asset",
      decimals: 6,
      initialSupply: parseUnits("1000000"),
    });
    // The node reports the id it assigned in the acknowledgement message.
    const tokenId = created.message?.match(/token_id=(\S+)/)?.[1];
    assert.ok(tokenId, `no token id in ${JSON.stringify(created)}`);
    const issued = await settle(client, async () => {
      const value = await client.assetBalance(tokenId).catch(() => 0n);
      return value === parseUnits("1000000") ? value : null;
    });
    assert.equal(issued, parseUnits("1000000"));

    // Seed the pool, then trade against it — both authorized by two
    // signatures, both settled by consensus with no contract and no bridge.
    await client.addLiquidity({
      tokenId,
      amountHkm: parseUnits("100"),
      amountToken: parseUnits("400"),
    });
    const pool = await settle(client, async () => {
      const value = await client.pool(tokenId).catch(() => null);
      return value?.reserve_hkm === parseUnits("100") ? value : null;
    });
    assert.equal(pool.reserve_hkm, parseUnits("100"));

    const before = await client.assetBalance(tokenId);
    await client.swap({ tokenId, hkmToToken: true, amountIn: parseUnits("5") });
    const after = await settle(client, async () => {
      const value = await client.assetBalance(tokenId);
      return value > before ? value : null;
    });
    assert.ok(after > before, "the swap did not deliver tokens to the hybrid account");
  });
});
