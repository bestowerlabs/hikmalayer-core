# @hikmalayer/sdk

The client library for Hikmalayer. Keys, native signing, transactions, native
tokens and the AMM DEX — with the parts that are easy to get catastrophically
wrong made hard to get wrong.

```bash
npm install @hikmalayer/sdk
```

## Quick start

```js
import { HikmalayerClient, LocalSigner, parseUnits, formatUnits } from "@hikmalayer/sdk";

const client = HikmalayerClient.withPrivateKey(process.env.HIKMALAYER_KEY, {
  url: "http://127.0.0.1:3000",
});

console.log(client.signer.address);                     // hkm…
console.log(formatUnits(await client.balance()), "HKM");

await client.transfer({
  to: "hkm0123…",
  amount: parseUnits("1.5"),   // 1_500_000n base units
});
```

Need a chain to talk to? `ops/devnet.sh` starts one, funded and mining, in a
single command.

## Networks

Every signature is bound to a network. Addresses come from the key, so the
same account exists on a testnet and on mainnet — without a network binding, a
transaction signed while testing replays verbatim against real funds.

The client discovers the network from the node and scopes every message it
signs:

```js
await client.resolveChainId();   // "hikmalayer-mainnet"
```

Pin it when you need certainty, since a node that reported the wrong id would
have you signing for the wrong chain:

```js
new HikmalayerClient({ url, signer, chainId: "hikmalayer-mainnet" });
```

Signing offline with the CLI? Set `HIKMALAYER_CHAIN_ID` to match, or the
signature is for the dev network and the node will refuse it.

## Two things this library exists to prevent

**Amounts are BigInt. Always.** HKM has 6 decimals and a 100-billion supply,
so the largest legitimate amount is 10^17 base units — well past
`Number.MAX_SAFE_INTEGER` (≈9.007×10^15). Routing such a value through
`Number` drops its low digits. That is worse than a display bug: signatures
cover the exact decimal text of an amount, so a rounded value produces a
transaction that no longer matches what was signed. The node rejects an
honest transfer and the error names nothing useful.

The SDK takes `BigInt`, decimal strings, or safe integers, and **refuses**
anything already corrupted:

```js
client.transfer({ to, amount: 9007199254740993 });  // throws: exceeds MAX_SAFE_INTEGER
client.transfer({ to, amount: 9007199254740993n }); // fine
client.transfer({ to, amount: "9007199254740993" }); // fine
```

**You never sign one thing and send another.** Every write method builds the
canonical message from the same values it puts on the wire, and resolves the
nonce once. There is no API for supplying a message and a payload separately,
because that is the shape of the mistake.

## Signing

Signatures are byte-identical to the `hikma-wallet` CLI and to the browser
wallet — verified in `test/conformance.test.mjs`, which signs every domain
with the real CLI and compares, including a non-ASCII case (the digest uses
UTF-8 **byte** length, not character count).

```js
import { LocalSigner, ExtensionSigner } from "@hikmalayer/sdk";

// Scripts, services, CI
const signer = new LocalSigner(process.env.HIKMALAYER_KEY);

// Browsers, via the wallet extension: the key never enters the page
const signer = new ExtensionSigner();
await signer.connect();
```

`LocalSigner` holds a key in process memory. That is right for a devnet, a
test, or a backend service. It is **not** right for a treasury or validator
key on a machine that browses the web — sign those offline with
`hikma-wallet`, or use the extension.

Any object with `address`, `publicKey` and `sign(message)` works as a signer,
so an HSM or remote signer drops straight in.

## Reads

Amount fields come back as `BigInt`, parsed from the raw response so large
values stay exact.

```js
await client.stats();                       // height, difficulty, validity
await client.state();                       // state root, supply, validators
await client.balance(address?);             // BigInt base units
await client.nextNonce(address?);
await client.assets();
await client.asset(tokenId);                // null if unknown
await client.assetBalance(tokenId, addr?);
await client.pools();
await client.pool(tokenId);
await client.lpPosition(tokenId, addr?);
await client.vesting(address?);
await client.quote(tokenId, { hkmToToken: true, amountIn });
```

## Writes

A submitted transaction is **queued**; it takes effect when mined into a
block. On a devnet, `client.mine()` seals one immediately.

```js
await client.transfer({ to, amount });
await client.stake({ amount, vrfPublicKey });
await client.withdrawStake({ amount });
await client.vest({ to, amount, cliffBlocks, durationBlocks });

await client.createAsset({ symbol: "MYT", name: "My Token", decimals: 6,
                           initialSupply: parseUnits("1000000", 6) });
await client.transferAsset({ tokenId, to, amount });
await client.burnAsset({ tokenId, amount });
```

## DEX

Slippage bounds are applied by default, from a live quote. Omitting them does
not mean "no bound" — an unbounded trade is a gift to whoever orders it — so
the default is 0.5%.

```js
// min_out comes from a quote, 0.5% tolerance
await client.swap({ tokenId, hkmToToken: true, amountIn: parseUnits("100") });

// tighter, or exact
await client.swap({ tokenId, amountIn, slippage: 0.1 });
await client.swap({ tokenId, amountIn, minOut: 4_950_000n });

await client.addLiquidity({ tokenId, amountHkm, amountToken });   // min_shares bounded
await client.removeLiquidity({ tokenId, shares });                // both sides bounded
```

Predict an outcome before committing to it. These mirror the chain's
arithmetic exactly — same rounding, same tie-breaks — and are verified against
a live pool in the integration suite:

```js
const pool = await client.pool(tokenId);
const { useHkm, useToken, minted } =
  HikmalayerClient.quoteAddLiquidity(pool, amountHkm, amountToken);

// The pool ratio binds on one side; the surplus stays in your account.
const { amountHkm, amountToken } =
  HikmalayerClient.quoteRemoveLiquidity(pool, shares);
```

## Errors

The node reports domain failures with HTTP 200 and `{"status":"error"}`, which
a naive client reads as success. The SDK raises `HikmalayerError` for those as
well as for HTTP failures, so a rejected transaction cannot be mistaken for an
accepted one.

```js
import { HikmalayerError } from "@hikmalayer/sdk";

try {
  await client.transfer({ to, amount });
} catch (error) {
  if (error instanceof HikmalayerError) {
    console.error(error.message, error.status, error.body);
  }
}
```

Recipient addresses are checked before anything is signed. There is no
checksum and no recovery: a mistyped address is simply a different account,
and crediting it puts the funds where nobody holds a key. The chain enforces
this too — the SDK just fails earlier, and says which field was wrong.

## Waiting for the chain

```js
const height = (await client.stats()).total_blocks;
await client.waitFor((stats) => stats.total_blocks > height);
```

## Testing

```bash
npm test                 # units + CLI conformance (offline)

ops/devnet.sh &          # in the repo root
HIKMALAYER_ADMIN_TOKEN=devadmin npm run test:integration
```

The conformance suite skips itself when `hikma-wallet` has not been built, and
the integration suite skips itself when no node is reachable, so `npm test`
works on a fresh checkout.

## API surface

| Export | What it is |
|---|---|
| `HikmalayerClient` | the node client |
| `HikmalayerError` | thrown for HTTP and domain failures |
| `LocalSigner`, `ExtensionSigner` | signers |
| `parseUnits`, `formatUnits`, `toBaseUnits`, `encodeAmount` | amounts |
| `applySlippage`, `isqrt` | DEX arithmetic |
| `messages` | canonical signing-message builders |
| `generatePrivateKey`, `derivePublicKey`, `deriveAddress` | key handling |
| `signMessage`, `verifyMessage`, `messageDigest` | raw signing |
| `isValidAddress`, `isValidPrivateKey` | validation |
