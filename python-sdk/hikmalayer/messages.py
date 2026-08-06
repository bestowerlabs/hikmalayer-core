"""
Canonical signing messages for every Hikmalayer transaction type.

Every signature is bound to a network (chain_id), so a transaction signed
against the devnet can never be replayed on testnet or mainnet. The chain
prefixes the domain with the network id before hashing:

    <chain_id>:<domain>

These functions must match src/blockchain/transaction.rs exactly. The
conformance tests in the JS SDK verify parity against the hikma-wallet CLI;
this module mirrors those same domains.
"""

from typing import Optional


def _amt(value) -> str:
    """Amounts are always the exact decimal text of an integer."""
    return str(int(value))


def transfer(from_addr: str, to_addr: str, amount, nonce: int) -> str:
    return f"hikmalayer-transfer:{from_addr}:{to_addr}:{_amt(amount)}:{int(nonce)}"


def stake(address: str, amount, nonce: int, vrf_public_key: Optional[str] = None) -> str:
    base = f"hikmalayer-stake:{address}:{_amt(amount)}:{int(nonce)}"
    return f"{base}:{vrf_public_key}" if vrf_public_key else base


def withdraw(address: str, amount, nonce: int) -> str:
    return f"hikmalayer-withdraw:{address}:{_amt(amount)}:{int(nonce)}"


def vest(from_addr: str, to_addr: str, amount, cliff_blocks: int,
         duration_blocks: int, nonce: int) -> str:
    return (
        f"hikmalayer-vest:{from_addr}:{to_addr}:{_amt(amount)}:"
        f"{int(cliff_blocks)}:{int(duration_blocks)}:{int(nonce)}"
    )


def token_create(symbol: str, name: str, decimals: int,
                 initial_supply, nonce: int) -> str:
    return (
        f"hikmalayer-token-create:{symbol}:{name}:{int(decimals)}:"
        f"{_amt(initial_supply)}:{int(nonce)}"
    )


def token_transfer(token_id: str, to_addr: str, amount, nonce: int) -> str:
    # The sender is established by the key that signs this, so it does not
    # appear in the domain. Verified against `hikma-wallet sign-token-transfer`.
    return f"hikmalayer-token-transfer:{token_id}:{to_addr}:{_amt(amount)}:{int(nonce)}"


def token_burn(token_id: str, amount, nonce: int) -> str:
    # As above: no sender in the domain.
    return f"hikmalayer-token-burn:{token_id}:{_amt(amount)}:{int(nonce)}"


def amm_add(token_id: str, amount_hkm, amount_token,
            min_shares, nonce: int) -> str:
    return (
        f"hikmalayer-amm-add:{token_id}:{_amt(amount_hkm)}:{_amt(amount_token)}:"
        f"{_amt(min_shares)}:{int(nonce)}"
    )


def amm_remove(token_id: str, shares, min_hkm, min_token, nonce: int) -> str:
    return (
        f"hikmalayer-amm-remove:{token_id}:{_amt(shares)}:{_amt(min_hkm)}:"
        f"{_amt(min_token)}:{int(nonce)}"
    )


def amm_swap(token_id: str, hkm_to_token: bool, amount_in,
             min_out, nonce: int) -> str:
    direction = "true" if hkm_to_token else "false"
    return (
        f"hikmalayer-amm-swap:{token_id}:{direction}:{_amt(amount_in)}:"
        f"{_amt(min_out)}:{int(nonce)}"
    )


def credential(cred_id: str, subject: str, data_hash: str,
               revoke: bool, nonce: int) -> str:
    flag = "true" if revoke else "false"
    return f"hikmalayer-credential:{cred_id}:{subject}:{data_hash}:{flag}:{int(nonce)}"


def scoped(chain_id: str, domain: str) -> str:
    """
    Bind a domain to a network. Every signature the node accepts is over
    this scoped form, so a signature is only ever valid on one chain.
    """
    return f"{chain_id}:{domain}"
