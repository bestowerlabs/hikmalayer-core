// Conformance: every signature this SDK produces must be byte-identical to
// the one the `hikma-wallet` CLI produces for the same inputs.
//
// This is the test that matters. A canonical message that has drifted from
// the Rust does not fail loudly — it produces a well-formed signature that
// the node simply refuses, and the error ("signature verification failed")
// names none of the dozen fields that could be wrong. Comparing against the
// real signer is the only way to know this file has not rotted.
//
// Skips itself if the CLI has not been built.

import { test, describe, before } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { LocalSigner, deriveAddress, derivePublicKey, signMessage } from "../src/index.js";
import { messages } from "../src/messages.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../..");
const CLI = [
  path.join(repo, "target/release/hikma-wallet"),
  path.join(repo, "target/debug/hikma-wallet"),
].find(existsSync);

// A fixed key so failures are reproducible.
const KEY = "80f91adc283392febbfc86b7327c055b8559373459040e07e78640e3ac592517";
const OTHER = "hkm0000000000000000000000000000000000000001";
const TOKEN = "hktcc3f73fed737c988826bc2540f1483bf8a640993";

/// Run the CLI and pull out the labelled fields it prints.
function cli(...args) {
  const output = execFileSync(CLI, args, { encoding: "utf8" });
  const field = (label) =>
    output.match(new RegExp(`^${label}:\\s*(\\S+)`, "m"))?.[1] ?? null;
  return {
    signature: field("signature"),
    publicKey: field("public_key"),
    message: output.match(/^message:\s*(.+)$/m)?.[1]?.trim() ?? null,
    address: field("address"),
  };
}

describe("parity with the hikma-wallet CLI", { skip: CLI ? false : "hikma-wallet not built" }, () => {
  let signer;
  let address;

  before(() => {
    signer = new LocalSigner(KEY);
    address = signer.address;
  });

  test("derives the same public key as the CLI", () => {
    // Any signing command prints the public key it derived from the key.
    const { publicKey } = cli("sign-transfer", "a", "b", "1", "0", KEY);
    assert.equal(signer.publicKey, publicKey);
    assert.equal(derivePublicKey(KEY), publicKey);
  });

  test("derives the same address as the CLI", () => {
    // `address` is the CLI's own hkm-address derivation.
    const { address: cliAddress } = cli("address", signer.publicKey);
    assert.equal(signer.address, cliAddress);
    assert.equal(deriveAddress(signer.publicKey), cliAddress);
  });

  // Each case: the CLI invocation, and the message the SDK builds for it.
  const cases = [
    {
      name: "transfer",
      args: () => ["sign-transfer", address, OTHER, "1500000", "7"],
      message: () => messages.transfer({ from: address, to: OTHER, amount: 1_500_000n, nonce: 7 }),
    },
    {
      name: "transfer of an amount beyond 2^53",
      args: () => ["sign-transfer", address, OTHER, "9007199254740993", "8"],
      message: () =>
        messages.transfer({ from: address, to: OTHER, amount: 9007199254740993n, nonce: 8 }),
    },
    {
      name: "stake (binds the VRF key)",
      args: () => ["sign-stake", address, "10000000000", "3", KEY],
      message: (out) =>
        messages.stake({
          address,
          amount: 10_000_000_000n,
          nonce: 3,
          // The CLI derives the VRF key from the private key and prints it.
          vrfPublicKey: out.message.split(":").at(-1),
        }),
      needsOutputFirst: true,
    },
    {
      name: "withdraw",
      args: () => ["sign-withdraw", address, "5000000", "4"],
      message: () => messages.withdraw({ address, amount: 5_000_000n, nonce: 4 }),
    },
    {
      name: "vest",
      args: () => ["sign-vest", address, OTHER, "2000000", "100", "1000", "5"],
      message: () =>
        messages.vest({
          from: address,
          to: OTHER,
          amount: 2_000_000n,
          cliffBlocks: 100,
          durationBlocks: 1000,
          nonce: 5,
        }),
    },
    {
      name: "token-create",
      args: () => ["sign-token-create", "TESTX", "Test Asset", "6", "1000000000000", "2"],
      message: () =>
        messages.tokenCreate({
          symbol: "TESTX",
          name: "Test Asset",
          decimals: 6,
          initialSupply: 1_000_000_000_000n,
          nonce: 2,
        }),
    },
    {
      // Byte length, not character count: 'é' and '本' are multi-byte, so a
      // digest built from `String.length` diverges here and nowhere else.
      name: "token-create with a non-ASCII name",
      args: () => ["sign-token-create", "CAFÉ", "Café 日本 ☕", "8", "42", "1"],
      message: () =>
        messages.tokenCreate({
          symbol: "CAFÉ",
          name: "Café 日本 ☕",
          decimals: 8,
          initialSupply: 42n,
          nonce: 1,
        }),
    },
    {
      name: "token-transfer",
      args: () => ["sign-token-transfer", TOKEN, OTHER, "250000", "9"],
      message: () =>
        messages.tokenTransfer({ tokenId: TOKEN, to: OTHER, amount: 250_000n, nonce: 9 }),
    },
    {
      name: "token-burn",
      args: () => ["sign-token-burn", TOKEN, "125000", "10"],
      message: () => messages.tokenBurn({ tokenId: TOKEN, amount: 125_000n, nonce: 10 }),
    },
    {
      name: "amm-add",
      args: () => ["sign-amm-add", TOKEN, "100000000000", "500000000000", "222488762765", "11"],
      message: () =>
        messages.ammAdd({
          tokenId: TOKEN,
          amountHkm: 100_000_000_000n,
          amountToken: 500_000_000_000n,
          minShares: 222_488_762_765n,
          nonce: 11,
        }),
    },
    {
      name: "amm-remove",
      args: () => ["sign-amm-remove", TOKEN, "100000000000", "44497752752", "222488763761", "12"],
      message: () =>
        messages.ammRemove({
          tokenId: TOKEN,
          shares: 100_000_000_000n,
          minHkm: 44_497_752_752n,
          minToken: 222_488_763_761n,
          nonce: 12,
        }),
    },
    {
      name: "amm-swap (hkm → token)",
      args: () => ["sign-amm-swap", TOKEN, "true", "100000000", "480000000", "13"],
      message: () =>
        messages.ammSwap({
          tokenId: TOKEN,
          hkmToToken: true,
          amountIn: 100_000_000n,
          minOut: 480_000_000n,
          nonce: 13,
        }),
    },
    {
      name: "amm-swap (token → hkm)",
      args: () => ["sign-amm-swap", TOKEN, "false", "500000000", "95000000", "14"],
      message: () =>
        messages.ammSwap({
          tokenId: TOKEN,
          hkmToToken: false,
          amountIn: 500_000_000n,
          minOut: 95_000_000n,
          nonce: 14,
        }),
    },
    {
      name: "credential (revoke=false)",
      args: () => ["sign-credential", "cert-1", "hkm-subject", "deadbeef", "false", "15"],
      message: () =>
        messages.credential({
          id: "cert-1",
          subject: "hkm-subject",
          dataHash: "deadbeef",
          revoke: false,
          nonce: 15,
        }),
    },
    {
      name: "credential (revoke=true)",
      args: () => ["sign-credential", "cert-1", "hkm-subject", "deadbeef", "true", "16"],
      message: () =>
        messages.credential({
          id: "cert-1",
          subject: "hkm-subject",
          dataHash: "deadbeef",
          revoke: true,
          nonce: 16,
        }),
    },
  ];

  for (const scenario of cases) {
    test(`${scenario.name}: message and signature match the CLI`, () => {
      const out = cli(...scenario.args(), KEY);
      const message = scenario.message(out);

      // The canonical message itself must match, so a failure says which
      // field is wrong instead of only "signatures differ".
      assert.equal(message, out.message, "canonical message differs from the CLI");

      const signature = signMessage(message, KEY);
      assert.equal(signature, out.signature, "signature differs from the CLI");
      assert.equal(signature.length, 128, "compact signature must be 64 bytes of hex");
    });
  }
});
