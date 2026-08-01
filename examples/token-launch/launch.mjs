#!/usr/bin/env node
//
// A token launch, end to end, on the Hikmalayer SDK.
//
//   ops/devnet.sh &                       # in another terminal
//   node examples/token-launch/launch.mjs
//
// This is the demo we use to check the developer tools actually work. It is
// not a toy loop of API calls: it plays out a full launch — issue an asset,
// seed a market, distribute to holders, lock the team allocation on a vesting
// schedule, let holders trade, then withdraw liquidity — and it VERIFIES the
// chain's arithmetic against its own predictions at each step. If the SDK and
// consensus ever disagree, this fails loudly rather than printing something
// plausible.
//
// Everything here uses the public SDK. There is no privileged access beyond
// the devnet faucet used to fund the demo accounts.

import {
  HikmalayerClient,
  LocalSigner,
  formatUnits,
  parseUnits,
} from "../../sdk/src/index.js";

const NODE = process.env.HIKMALAYER_NODE ?? "http://127.0.0.1:3000";
const ADMIN = process.env.HIKMALAYER_ADMIN_TOKEN ?? "devadmin";

const HKM = (units) => `${formatUnits(units)} HKM`;

let checks = 0;
let failures = 0;

/// Assert, and say what was being asserted. A demo that silently prints the
/// wrong number is worse than no demo.
function expect(label, actual, expected) {
  checks += 1;
  const ok = actual === expected;
  if (!ok) failures += 1;
  console.log(
    `      ${ok ? "✓" : "✗"} ${label}` +
      (ok ? "" : `\n        expected ${expected}\n        actual   ${actual}`)
  );
}

const step = (n, title) => console.log(`\n[${n}] ${title}`);

// ---------------------------------------------------------------- setup

const admin = new HikmalayerClient({ url: NODE, adminToken: ADMIN });

/// Queued transactions execute when mined. The devnet mines on a timer, but
/// waiting for that would make this demo slow and flaky, so it seals a block
/// itself and waits for the height to move.
async function settle() {
  const before = (await admin.stats()).total_blocks;
  await admin.mine().catch(() => {});
  await admin.waitFor((s) => s.total_blocks > before, {
    timeoutMs: 30_000,
    intervalMs: 250,
  });
}

/// A funded participant.
async function participant(name, hkm) {
  const client = new HikmalayerClient({ url: NODE, signer: LocalSigner.random() });
  await admin.faucet({ to: client.signer.address, amount: hkm });
  return Object.assign(client, { name });
}

console.log("Hikmalayer token launch demo");
console.log("=".repeat(60));

const chain = await admin.state();
console.log(`Node     ${NODE}`);
console.log(`Network  ${chain.chain_id}`);
console.log(`Height   ${chain.height}`);
console.log(`Supply   ${HKM(chain.total_supply)}`);

// ---------------------------------------------------------------- cast

step(1, "Funding the participants");

const issuer = await participant("Issuer", parseUnits("500000"));
const alice = await participant("Alice", parseUnits("20000"));
const bob = await participant("Bob", parseUnits("20000"));
const team = await participant("Team", parseUnits("10"));
await settle();

for (const who of [issuer, alice, bob, team]) {
  console.log(`      ${who.name.padEnd(7)} ${who.signer.address}  ${HKM(await who.balance())}`);
}
expect("issuer funded", await issuer.balance(), parseUnits("500000"));

// ---------------------------------------------------------------- issue

step(2, "Issuing the token");

const SUPPLY = parseUnits("1000000", 6); // 1,000,000 LUNA
const created = await issuer.createAsset({
  symbol: "LUNA",
  name: "Luna Protocol",
  decimals: 6,
  initialSupply: SUPPLY,
});
const tokenId = created.message.match(/token_id=(\S+)/)[1];
await settle();

const asset = await issuer.asset(tokenId);
console.log(`      ${asset.symbol} (${asset.name})  ${tokenId}`);
console.log(`      supply ${formatUnits(asset.total_supply, asset.decimals)} ${asset.symbol}`);
expect("whole supply minted to the issuer", await issuer.assetBalance(tokenId), SUPPLY);

// ---------------------------------------------------------------- vesting

step(3, "Locking the team allocation (20% on a cliff + linear schedule)");

const TEAM_ALLOCATION = parseUnits("100000"); // in HKM, from the issuer
await issuer.vest({
  to: team.signer.address,
  amount: TEAM_ALLOCATION,
  cliffBlocks: 10,
  durationBlocks: 40,
});
await settle();

console.log(`      locked ${HKM(TEAM_ALLOCATION)} for ${team.signer.address}`);
console.log(`      team spendable now: ${HKM(await team.balance())}`);
expect(
  "nothing released before the cliff",
  (await team.balance()) < parseUnits("11"),
  true
);

// ---------------------------------------------------------------- market

step(4, "Seeding the market (the launch price is set here)");

const POOL_HKM = parseUnits("100000");
const POOL_TOKEN = parseUnits("500000", 6); // 5 LUNA per HKM

// Predict first, then act, then check the chain agreed.
const predicted = HikmalayerClient.quoteAddLiquidity(null, POOL_HKM, POOL_TOKEN);
await issuer.addLiquidity({
  tokenId,
  amountHkm: POOL_HKM,
  amountToken: POOL_TOKEN,
});
await settle();

const pool = await issuer.pool(tokenId);
const position = await issuer.lpPosition(tokenId);
console.log(`      reserves ${HKM(pool.reserve_hkm)} / ${formatUnits(pool.reserve_token, 6)} LUNA`);
console.log(`      LP shares ${position.shares}`);
console.log(`      launch price 1 HKM = ${formatUnits((pool.reserve_token * 1_000_000n) / pool.reserve_hkm, 6)} LUNA`);
expect("shares minted match the SDK's prediction", position.shares, predicted.minted);
expect("HKM reserve", pool.reserve_hkm, POOL_HKM);

// ---------------------------------------------------------------- trading

step(5, "Holders trade against the pool");

const aliceSpend = parseUnits("1000");
const quote = await alice.quote(tokenId, { hkmToToken: true, amountIn: aliceSpend });
const aliceTokensBefore = await alice.assetBalance(tokenId);
await alice.swap({ tokenId, hkmToToken: true, amountIn: aliceSpend, slippage: 0.5 });
await settle();

const aliceGained = (await alice.assetBalance(tokenId)) - aliceTokensBefore;
console.log(`      Alice paid ${HKM(aliceSpend)}, received ${formatUnits(aliceGained, 6)} LUNA`);
expect("received exactly the quoted amount", aliceGained, quote.amount_out);

// Bob buys after Alice, so he pays a worse price — that is the curve working.
const bobQuote = await bob.quote(tokenId, { hkmToToken: true, amountIn: aliceSpend });
await bob.swap({ tokenId, hkmToToken: true, amountIn: aliceSpend });
await settle();
const bobGained = await bob.assetBalance(tokenId);
console.log(`      Bob   paid ${HKM(aliceSpend)}, received ${formatUnits(bobGained, 6)} LUNA`);
expect("the second buyer of the same size gets less", bobGained < aliceGained, true);
expect("Bob received exactly his quote", bobGained, bobQuote.amount_out);

// And Alice sells some back.
const sellAmount = aliceGained / 2n;
const hkmBefore = await alice.balance();
await alice.swap({ tokenId, hkmToToken: false, amountIn: sellAmount });
await settle();
console.log(
  `      Alice sold ${formatUnits(sellAmount, 6)} LUNA for ${HKM((await alice.balance()) - hkmBefore)}`
);

// ---------------------------------------------------------------- fees

step(6, "Checking the pool invariant (fees must accrue to LPs)");

const afterTrading = await issuer.pool(tokenId);
const kStart = POOL_HKM * POOL_TOKEN;
const kNow = afterTrading.reserve_hkm * afterTrading.reserve_token;
console.log(`      k at launch  ${kStart}`);
console.log(`      k now        ${kNow}`);
const growthPpm = Number((kNow * 1_000_000_000n) / kStart - 1_000_000_000n) / 10_000_000;
console.log(`      growth       +${growthPpm.toFixed(4)}% (the 0.30% swap fee, retained for LPs)`);
expect("the constant product never shrinks", kNow > kStart, true);

// ---------------------------------------------------------------- distribution

step(7, "Distributing tokens to holders");

const airdrop = parseUnits("2500", 6);
await issuer.transferAsset({ tokenId, to: bob.signer.address, amount: airdrop });
await settle();
console.log(`      sent ${formatUnits(airdrop, 6)} LUNA to Bob`);
expect("airdrop landed", (await bob.assetBalance(tokenId)) - bobGained, airdrop);

// A malformed address must be caught before anything is signed.
let refused = false;
try {
  await issuer.transferAsset({ tokenId, to: "hkm-typo", amount: airdrop });
} catch {
  refused = true;
}
expect("a mistyped recipient is refused, not silently burned", refused, true);

// ---------------------------------------------------------------- exit

step(8, "Issuer withdraws half the liquidity");

const before = await issuer.pool(tokenId);
const shares = (await issuer.lpPosition(tokenId)).shares / 2n;
const expected = HikmalayerClient.quoteRemoveLiquidity(before, shares);
const issuerHkmBefore = await issuer.balance();

await issuer.removeLiquidity({ tokenId, shares });
await settle();

const after = await issuer.pool(tokenId);
console.log(`      burned ${shares} shares`);
console.log(
  `      received ${HKM(before.reserve_hkm - after.reserve_hkm)} + ` +
    `${formatUnits(before.reserve_token - after.reserve_token, 6)} LUNA`
);
expect("HKM out matches the prediction", before.reserve_hkm - after.reserve_hkm, expected.amountHkm);
expect("token out matches the prediction", before.reserve_token - after.reserve_token, expected.amountToken);
expect("issuer's HKM increased", (await issuer.balance()) > issuerHkmBefore, true);

// ---------------------------------------------------------------- vesting release

step(9, "Team allocation releases as blocks pass");

const teamBefore = await team.balance();
for (let i = 0; i < 8; i += 1) await settle();
const teamAfter = await team.balance();
console.log(`      before ${HKM(teamBefore)} → after ${HKM(teamAfter)}`);
expect("tokens released after the cliff", teamAfter > teamBefore, true);
expect("but not more than was locked", teamAfter <= TEAM_ALLOCATION + parseUnits("10"), true);

// ---------------------------------------------------------------- summary

step(10, "Final state");

const finalPool = await issuer.pool(tokenId);
const finalAsset = await issuer.asset(tokenId);
console.log(`      ${finalAsset.symbol} supply   ${formatUnits(finalAsset.total_supply, 6)}`);
console.log(`      pool           ${HKM(finalPool.reserve_hkm)} / ${formatUnits(finalPool.reserve_token, 6)} LUNA`);
console.log(`      holders        issuer ${formatUnits(await issuer.assetBalance(tokenId), 6)}, ` +
  `alice ${formatUnits(await alice.assetBalance(tokenId), 6)}, ` +
  `bob ${formatUnits(await bob.assetBalance(tokenId), 6)}`);
console.log(`      chain height   ${(await admin.stats()).total_blocks}`);
console.log(`      chain valid    ${(await admin.stats()).is_valid}`);

console.log("\n" + "=".repeat(60));
console.log(`${checks - failures}/${checks} checks passed`);
if (failures) {
  console.log(`${failures} FAILED — the SDK and the chain disagree.`);
  process.exit(1);
}
console.log("The SDK's predictions matched the chain at every step.");
