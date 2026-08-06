// @hikmalayer/sdk — the developer entry point.
//
//   import { HikmalayerClient, LocalSigner, parseUnits } from "@hikmalayer/sdk";
//
//   const client = HikmalayerClient.withPrivateKey(process.env.KEY, {
//     url: "http://127.0.0.1:3000",
//   });
//
//   await client.transfer({ to: "hkm…", amount: parseUnits("1.5") });
//
// Amounts are BigInt base units throughout. See src/units.js for why that is
// not negotiable.

export { HikmalayerClient, HikmalayerError, MINIMUM_LIQUIDITY } from "./client.js";

export {
  ExtensionSigner,
  LocalSigner,
  deriveAddress,
  derivePublicKey,
  generatePrivateKey,
  isValidAddress,
  isValidPrivateKey,
  messageDigest,
  normalizeHex,
  signMessage,
  verifyMessage,
} from "./signer.js";

export {
  HKM_DECIMALS,
  UNITS_PER_HKM,
  applySlippage,
  encodeAmount,
  formatUnits,
  isqrt,
  parseUnits,
  toBaseUnits,
} from "./units.js";

export {
  ADDRESS_PREFIX,
  MESSAGE_PREFIX,
  messages,
  scoped,
} from "./messages.js";

// Quantum-ready accounts. Both signatures required; see src/hybrid.js.
export {
  HYBRID_ADDRESS_PREFIX,
  HybridSigner,
  PQ_PUBLIC_KEY_LEN,
  PQ_SIGNATURE_LEN,
  derivePqPublicKey,
  derivePqSeed,
  deriveHybridAddress,
  deriveHybridIdentity,
  isHybridAddress,
  isValidPqPublicKey,
  pqSignMessage,
  pqVerifyMessage,
  verifyHybrid,
} from "./hybrid.js";
