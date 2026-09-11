"""
hikmalayer — the official Python SDK for the Hikmalayer blockchain.

Keys, native signing, transactions, tokens and the AMM DEX.

    from hikmalayer import HikmalayerClient, LocalSigner, parse_hkm, format_hkm

    signer = LocalSigner.random()
    client = HikmalayerClient("http://127.0.0.1:3000", signer=signer)

    print(client.stats())
    client.transfer(to="hkm…", amount=parse_hkm("10"))

Amounts are always integer BASE UNITS (1 HKM = 1_000_000). Use parse_hkm
to convert from a human decimal string, and format_hkm to convert back.
"""

from .client import HikmalayerClient, HikmalayerError
from .signer import (
    LocalSigner,
    derive_address,
    derive_public_key,
    is_valid_address,
    is_valid_token_id,
    sign_message,
    sign_digest,
    verify_message,
    ADDRESS_PREFIX,
    TOKEN_PREFIX,
)
from .units import (
    HKM_DECIMALS,
    HKM_SCALE,
    parse_units,
    format_units,
    parse_hkm,
    format_hkm,
    to_base_units,
    apply_slippage,
    isqrt,
)
from . import messages
from . import hybrid
from .hybrid import (
    HybridSigningUnavailable,
    derive_hybrid_address,
    derive_hybrid_identity,
    derive_pq_public_key,
    is_hybrid_address,
    pq_available,
    pq_verify_message,
    verify_hybrid,
)

__version__ = "0.1.0"

__all__ = [
    "HikmalayerClient",
    "HikmalayerError",
    "LocalSigner",
    "derive_address",
    "derive_public_key",
    "is_valid_address",
    "is_valid_token_id",
    "sign_message",
    "sign_digest",
    "verify_message",
    "ADDRESS_PREFIX",
    "TOKEN_PREFIX",
    "HKM_DECIMALS",
    "HKM_SCALE",
    "parse_units",
    "format_units",
    "parse_hkm",
    "format_hkm",
    "to_base_units",
    "apply_slippage",
    "isqrt",
    "messages",
    "hybrid",
    "HybridSigningUnavailable",
    "derive_hybrid_address",
    "derive_hybrid_identity",
    "derive_pq_public_key",
    "is_hybrid_address",
    "pq_available",
    "pq_verify_message",
    "verify_hybrid",
]
