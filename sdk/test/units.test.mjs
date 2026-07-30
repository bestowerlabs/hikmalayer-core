import { test, describe } from "node:test";
import assert from "node:assert/strict";

import {
  applySlippage,
  encodeAmount,
  formatUnits,
  isqrt,
  parseUnits,
  toBaseUnits,
} from "../src/units.js";
import { HikmalayerClient, MINIMUM_LIQUIDITY } from "../src/client.js";

describe("amount coercion", () => {
  test("accepts BigInt, safe integers and digit strings", () => {
    assert.equal(toBaseUnits(1_500_000n), 1_500_000n);
    assert.equal(toBaseUnits(1_500_000), 1_500_000n);
    assert.equal(toBaseUnits("1500000"), 1_500_000n);
    assert.equal(toBaseUnits("  42  "), 42n);
  });

  test("refuses a number that has already lost precision", () => {
    // 2^53 + 1 is not representable; the literal below IS 2^53. Accepting it
    // would mean signing an amount the caller never wrote.
    assert.throws(() => toBaseUnits(9007199254740993), /MAX_SAFE_INTEGER/);
    assert.throws(() => toBaseUnits(1e17), /MAX_SAFE_INTEGER/);
  });

  test("refuses fractional numbers rather than rounding", () => {
    assert.throws(() => toBaseUnits(1.5), /whole number/);
    assert.throws(() => toBaseUnits("1.5"), /parseUnits/);
  });

  test("carries amounts past 2^53 exactly", () => {
    const supply = "100000000000000000"; // 100B HKM in base units
    assert.equal(toBaseUnits(supply), 100_000_000_000_000_000n);
    assert.equal(encodeAmount(supply), supply);
    // One unit less must remain a distinct value.
    assert.notEqual(toBaseUnits("99999999999999999"), toBaseUnits(supply));
  });
});

describe("parseUnits / formatUnits", () => {
  test("round-trips decimal amounts", () => {
    assert.equal(parseUnits("1.5"), 1_500_000n);
    assert.equal(parseUnits("0.000001"), 1n);
    assert.equal(parseUnits("100"), 100_000_000n);
    assert.equal(formatUnits(1_500_000n), "1.5");
    assert.equal(formatUnits(1n), "0.000001");
    assert.equal(formatUnits(100_000_000n), "100");
  });

  test("honours a token's own decimals", () => {
    assert.equal(parseUnits("1.5", 18), 1_500_000_000_000_000_000n);
    assert.equal(formatUnits(1_500_000_000_000_000_000n, 18), "1.5");
    assert.equal(parseUnits("7", 0), 7n);
    assert.equal(formatUnits(7n, 0), "7");
  });

  test("rejects more precision than the asset has", () => {
    assert.throws(() => parseUnits("1.0000001"), /6 decimals/);
  });

  test("rejects junk", () => {
    assert.throws(() => parseUnits("abc"));
    assert.throws(() => parseUnits("1.2.3"));
    assert.throws(() => parseUnits("."));
  });

  test("handles the whole supply without loss", () => {
    const all = parseUnits("100000000000"); // 100 billion HKM
    assert.equal(all, 100_000_000_000_000_000n);
    assert.equal(formatUnits(all), "100000000000");
  });
});

describe("slippage", () => {
  test("applies a percentage in basis points", () => {
    assert.equal(applySlippage(1_000_000n, 0.5), 995_000n);
    assert.equal(applySlippage(1_000_000n, 1), 990_000n);
    assert.equal(applySlippage(1_000_000n, 0), 1_000_000n);
  });

  test("a 100% tolerance means no floor at all", () => {
    assert.equal(applySlippage(1_000_000n, 100), 0n);
  });

  test("stays exact on amounts beyond the float range", () => {
    assert.equal(applySlippage(100_000_000_000_000_000n, 0.5), 99_500_000_000_000_000n);
  });
});

describe("isqrt", () => {
  test("matches the chain's integer square root", () => {
    assert.equal(isqrt(0n), 0n);
    assert.equal(isqrt(1n), 1n);
    assert.equal(isqrt(4n), 2n);
    assert.equal(isqrt(8n), 2n); // floor
    assert.equal(isqrt(10n ** 22n), 10n ** 11n);
  });

  test("never overestimates", () => {
    for (const n of [2n, 3n, 99n, 12345n, 5n * 10n ** 22n]) {
      const root = isqrt(n);
      assert.ok(root * root <= n, `${root}^2 > ${n}`);
      assert.ok((root + 1n) * (root + 1n) > n, `${root + 1n}^2 <= ${n}`);
    }
  });
});

describe("liquidity quoting", () => {
  const pool = {
    reserve_hkm: 100_000_000_000n,
    reserve_token: 500_000_000_000n,
    total_shares: 223_606_797_749n,
  };

  test("first deposit mints sqrt(x*y) minus the locked minimum", () => {
    const quote = HikmalayerClient.quoteAddLiquidity(null, 100_000_000_000n, 500_000_000_000n);
    assert.equal(quote.first, true);
    assert.equal(quote.minted, isqrt(100_000_000_000n * 500_000_000_000n) - MINIMUM_LIQUIDITY);
    // Value confirmed against a live node in the integration suite.
    assert.equal(quote.minted, 223_606_796_749n);
  });

  test("a first deposit too small to clear the minimum mints nothing", () => {
    assert.equal(HikmalayerClient.quoteAddLiquidity(null, 100n, 100n), null);
  });

  test("later deposits are bound by the scarcer side", () => {
    // Offering token far beyond the ratio: HKM binds, surplus token is left.
    const quote = HikmalayerClient.quoteAddLiquidity(pool, 10_000_000_000n, 999_000_000_000n);
    assert.equal(quote.useHkm, 10_000_000_000n);
    assert.equal(quote.useToken, 50_000_000_000n); // the pool ratio, not the offer
    assert.equal(quote.minted, 22_360_679_774n);
  });

  test("the other side can bind too", () => {
    const quote = HikmalayerClient.quoteAddLiquidity(pool, 999_000_000_000n, 50_000_000_000n);
    assert.equal(quote.useToken, 50_000_000_000n);
    assert.equal(quote.useHkm, 10_000_000_000n);
  });

  test("removal returns the pro-rata share of both reserves", () => {
    const quote = HikmalayerClient.quoteRemoveLiquidity(pool, 100_000_000_000n);
    assert.equal(quote.amountHkm, 44_721_359_550n);
    assert.equal(quote.amountToken, 223_606_797_750n);
  });

  test("burning nothing withdraws nothing", () => {
    assert.equal(HikmalayerClient.quoteRemoveLiquidity(pool, 0n), null);
  });
});
