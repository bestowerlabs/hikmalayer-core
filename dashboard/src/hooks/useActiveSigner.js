// Chooses the signer to use, in descending order of safety:
//
//   1. The Hikmalayer Wallet EXTENSION — the key lives outside this page, so
//      an XSS here cannot reach it and approvals happen in extension UI.
//   2. The in-page wallet — convenient, hardened, but the key is in this page
//      while unlocked.
//   3. Neither: components fall back to the offline `hikma-wallet` CLI flow,
//      which is what cold keys should use anyway.
import { useSigner } from "./useSigner";
import { useExtension } from "./useExtension";

export function useActiveSigner() {
  const extension = useExtension();
  const vault = useSigner();

  if (extension.available && extension.connected) {
    return {
      canSign: true,
      source: "extension",
      sign: extension.sign,
      publicKey: extension.publicKey,
      account: extension.account,
      extension,
      vault,
    };
  }

  if (vault.unlocked) {
    return {
      canSign: true,
      source: "wallet",
      sign: vault.sign,
      publicKey: vault.publicKey,
      account: vault.address,
      extension,
      vault,
    };
  }

  return {
    canSign: false,
    source: extension.available ? "extension-available" : "none",
    sign: async () => {
      throw new Error("No signer available");
    },
    publicKey: null,
    account: null,
    extension,
    vault,
  };
}
