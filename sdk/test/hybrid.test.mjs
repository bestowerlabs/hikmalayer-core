// Hybrid (quantum-ready) accounts: parity with the node, and the security
// properties the scheme is supposed to have.
//
// The parity half matters most. ML-DSA is deterministic here on purpose so
// that this file and `src/consensus/pq.rs` produce identical bytes; if they
// ever diverge, a hybrid transaction signed in JavaScript is simply refused
// on chain, and the error names nothing useful. Comparing against the real
// signer is the only way to know.

import { test, describe, before } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { secp256k1 } from "@noble/curves/secp256k1";
import { bytesToHex } from "@noble/hashes/utils";

import {
  HYBRID_ADDRESS_PREFIX,
  HybridSigner,
  LocalSigner,
  PQ_PUBLIC_KEY_LEN,
  PQ_SIGNATURE_LEN,
  derivePqPublicKey,
  derivePqSeed,
  deriveHybridAddress,
  deriveHybridIdentity,
  isCanonicalPqPublicKey,
  isCanonicalPublicKey,
  isHybridAddress,
  isValidAddress,
  isValidPqPublicKey,
  pqSignMessage,
  pqVerifyMessage,
  signMessage,
  verifyHybrid,
} from "../src/index.js";
import { messages, scoped } from "../src/messages.js";

const CHAIN_ID = "hikmalayer-conformance";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../..");
const CLI = [
  path.join(repo, "target/release/hikma-wallet"),
  path.join(repo, "target/debug/hikma-wallet"),
].find(existsSync);

const KEY = "80f91adc283392febbfc86b7327c055b8559373459040e07e78640e3ac592517";
const OTHER_KEY = "1111111111111111111111111111111111111111111111111111111111111111";
const PAYEE = "hkm0000000000000000000000000000000000000001";

/// Run the CLI with hybrid output enabled and pull out the labelled fields.
function cli(...args) {
  const output = execFileSync(CLI, args, {
    encoding: "utf8",
    env: { ...process.env, HIKMALAYER_CHAIN_ID: CHAIN_ID, HIKMALAYER_HYBRID: "1" },
    // A hybrid signature is ~6.6 KB of hex; the default pipe is fine but be
    // explicit so a larger case later does not truncate silently.
    maxBuffer: 4 * 1024 * 1024,
  });
  const field = (label) => output.match(new RegExp(`^${label}:\\s*(\\S+)`, "m"))?.[1] ?? null;
  return {
    message: output.match(/^message:\s*(.+)$/m)?.[1]?.trim() ?? null,
    publicKey: field("public_key"),
    signature: field("signature"),
    pqPublicKey: field("pq_public_key"),
    pqSignature: field("pq_signature"),
    hybridAddress: field("hybrid_address"),
    address: field("address"),
  };
}

describe("hybrid identity", () => {
  test("derives deterministically from the one secret the user already has", () => {
    const first = deriveHybridIdentity(KEY);
    const second = deriveHybridIdentity(KEY);
    assert.deepEqual(first, second);
    assert.ok(first.address.startsWith(HYBRID_ADDRESS_PREFIX));
    assert.equal(first.address.length, 43);
    assert.equal(first.pqPublicKey.length, PQ_PUBLIC_KEY_LEN * 2);
  });

  test("the post-quantum seed is domain-separated from the master secret", () => {
    // Reusing the raw secret across two schemes would make one break into
    // two.
    assert.notEqual(Buffer.from(derivePqSeed(KEY)).toString("hex"), KEY);
  });

  test("the hybrid and classical accounts of one key are different accounts", () => {
    const hybrid = deriveHybridIdentity(KEY);
    const classical = new LocalSigner(KEY);
    assert.notEqual(hybrid.address, classical.address);
    assert.equal(hybrid.publicKey, classical.publicKey);
    assert.ok(isHybridAddress(hybrid.address));
    assert.ok(!isHybridAddress(classical.address));
    // Both are payable, which is the point of accepting either prefix.
    assert.ok(isValidAddress(hybrid.address));
    assert.ok(isValidAddress(classical.address));
  });

  test("refuses to build an address from keys it cannot verify", () => {
    const id = deriveHybridIdentity(KEY);
    assert.throws(() => deriveHybridAddress("not-hex", id.pqPublicKey));
    assert.throws(() => deriveHybridAddress(id.publicKey, "not-hex"));
    // Syntactically fine, not a point on the curve: this must never become an
    // address, because nobody could ever sign for it.
    assert.throws(() => deriveHybridAddress("04".repeat(65), id.pqPublicKey));
    assert.ok(!isValidPqPublicKey(""));
    assert.ok(!isValidPqPublicKey("aabb"));
    assert.ok(isValidPqPublicKey(id.pqPublicKey));
  });

  test("a key has exactly one spelling", () => {
    // A compressed encoding is the SAME key: it verifies the same signatures
    // and would hash to the same address. Accepting it would give one
    // authorized transaction two valid encodings, and two transaction ids.
    const id = deriveHybridIdentity(KEY);
    const compressed = bytesToHex(
      secp256k1.ProjectivePoint.fromHex(id.publicKey).toRawBytes(true)
    );
    assert.notEqual(compressed, id.publicKey);
    assert.ok(isCanonicalPublicKey(id.publicKey));
    assert.ok(!isCanonicalPublicKey(compressed));
    assert.ok(!isCanonicalPublicKey(id.publicKey.toUpperCase()));
    assert.throws(() => deriveHybridAddress(compressed, id.pqPublicKey));
    assert.throws(() => deriveHybridAddress(id.publicKey, id.pqPublicKey.toUpperCase()));
    assert.ok(!isCanonicalPqPublicKey(id.pqPublicKey.toUpperCase()));
    assert.ok(isCanonicalPqPublicKey(id.pqPublicKey));
  });

  test("rejects a malformed private key", () => {
    assert.throws(() => new HybridSigner("nope"));
    assert.throws(() => deriveHybridIdentity(""));
  });
});

describe("hybrid signing", () => {
  test("signs and verifies under both schemes", async () => {
    const signer = new HybridSigner(KEY);
    const signature = await signer.sign("pay alice 10");
    assert.equal(signature.pqSignature.length, PQ_SIGNATURE_LEN * 2);
    assert.ok(
      verifyHybrid(
        signer.address,
        "pay alice 10",
        signer.publicKey,
        signer.pqPublicKey,
        signature.signature,
        signature.pqSignature
      )
    );
  });

  test("post-quantum signing is deterministic", () => {
    assert.equal(pqSignMessage("same message", KEY), pqSignMessage("same message", KEY));
  });

  test("a tampered message does not verify", async () => {
    const signer = new HybridSigner(KEY);
    const signature = await signer.sign("pay alice 10");
    assert.ok(
      !verifyHybrid(
        signer.address,
        "pay alice 1000",
        signer.publicKey,
        signer.pqPublicKey,
        signature.signature,
        signature.pqSignature
      )
    );
  });

  test("malformed post-quantum input returns false instead of throwing", () => {
    const id = deriveHybridIdentity(KEY);
    const signature = pqSignMessage("m", KEY);
    assert.ok(!pqVerifyMessage("m", "not-hex", signature));
    assert.ok(!pqVerifyMessage("m", id.pqPublicKey, "not-hex"));
    assert.ok(!pqVerifyMessage("m", "", ""));
    assert.ok(!pqVerifyMessage("m", id.pqPublicKey, signature.slice(0, -2)));
  });
});

// The properties that are the entire reason for the scheme. If any of these
// pass where they should fail, hybrid has collapsed back to one algorithm.
describe("hybrid resists a break of either scheme", () => {
  test("a valid classical signature alone cannot authorize a hybrid account", () => {
    // What an attacker holding a broken secp256k1 would have: a genuine
    // ECDSA signature, and nothing for ML-DSA.
    const id = deriveHybridIdentity(KEY);
    assert.ok(
      !verifyHybrid(
        id.address,
        "drain",
        id.publicKey,
        id.pqPublicKey,
        signMessage("drain", KEY),
        "00".repeat(PQ_SIGNATURE_LEN)
      )
    );
  });

  test("a valid post-quantum signature alone cannot authorize a hybrid account", () => {
    const id = deriveHybridIdentity(KEY);
    assert.ok(
      !verifyHybrid(
        id.address,
        "drain",
        id.publicKey,
        id.pqPublicKey,
        "00".repeat(64),
        pqSignMessage("drain", KEY)
      )
    );
  });

  test("substituting the attacker's post-quantum key names a different account", () => {
    // The realistic attack: secp256k1 is broken, so the attacker can sign
    // with the victim's classical key, and supplies an ML-DSA key of their
    // own. Binding both keys into the address is what defeats it.
    const victim = deriveHybridIdentity(KEY);
    const attacker = deriveHybridIdentity(OTHER_KEY);
    assert.ok(
      !verifyHybrid(
        victim.address,
        "drain",
        victim.publicKey,
        attacker.pqPublicKey,
        signMessage("drain", KEY),
        pqSignMessage("drain", OTHER_KEY)
      )
    );
  });

  test("substituting the attacker's classical key names a different account", () => {
    const victim = deriveHybridIdentity(KEY);
    const attacker = deriveHybridIdentity(OTHER_KEY);
    assert.ok(
      !verifyHybrid(
        victim.address,
        "drain",
        attacker.publicKey,
        victim.pqPublicKey,
        signMessage("drain", OTHER_KEY),
        pqSignMessage("drain", KEY)
      )
    );
  });

  test("another account's signatures do not verify", () => {
    const id = deriveHybridIdentity(KEY);
    assert.ok(!pqVerifyMessage("m", derivePqPublicKey(OTHER_KEY), pqSignMessage("m", KEY)));
    assert.ok(
      !verifyHybrid(
        id.address,
        "m",
        id.publicKey,
        id.pqPublicKey,
        signMessage("m", OTHER_KEY),
        pqSignMessage("m", KEY)
      )
    );
  });
});

describe(
  "parity with the hikma-wallet CLI",
  { skip: CLI ? false : "hikma-wallet not built" },
  () => {
    let signer;

    before(() => {
      signer = new HybridSigner(KEY);
    });

    test("derives the same hybrid address and post-quantum key as the node", () => {
      const out = cli("identity", KEY);
      assert.equal(signer.address, out.hybridAddress);
      assert.equal(signer.pqPublicKey, out.pqPublicKey);
    });

    test("produces byte-identical signatures for a transfer", async () => {
      const out = cli(
        "sign-transfer",
        signer.address,
        PAYEE,
        "1500000",
        "7",
        KEY
      );
      const message = scoped(
        CHAIN_ID,
        messages.transfer({ from: signer.address, to: PAYEE, amount: 1_500_000n, nonce: 7 })
      );
      assert.equal(message, out.message, "canonical message differs from the CLI");

      const signature = await signer.sign(message);
      assert.equal(signature.signature, out.signature, "classical signature differs");
      assert.equal(signature.pqSignature, out.pqSignature, "post-quantum signature differs");
      assert.equal(signature.pqSignature.length, PQ_SIGNATURE_LEN * 2);
    });

    test("produces byte-identical signatures for a swap", async () => {
      const token = "hktcc3f73fed737c988826bc2540f1483bf8a640993";
      const out = cli("sign-amm-swap", token, "true", "100000000", "480000000", "13", KEY);
      const message = scoped(
        CHAIN_ID,
        messages.ammSwap({
          tokenId: token,
          hkmToToken: true,
          amountIn: 100_000_000n,
          minOut: 480_000_000n,
          nonce: 13,
        })
      );
      assert.equal(message, out.message);
      const signature = await signer.sign(message);
      assert.equal(signature.signature, out.signature);
      assert.equal(signature.pqSignature, out.pqSignature);
    });

    test("a node signature verifies against the SDK, and the reverse", async () => {
      const out = cli("sign-transfer", signer.address, PAYEE, "1", "0", KEY);
      assert.ok(
        verifyHybrid(
          out.hybridAddress,
          out.message,
          out.publicKey,
          out.pqPublicKey,
          out.signature,
          out.pqSignature
        )
      );
    });
  }
);
