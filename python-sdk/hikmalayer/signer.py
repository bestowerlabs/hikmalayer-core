"""
Key handling and native signing for Hikmalayer.

The private key never leaves this process. Signing reproduces the chain's
scheme exactly (see src/consensus/pos.rs):

    address   = "hkm" + hex(SHA256(uncompressed_pubkey)[:20])
    digest    = SHA256(b"\\x19Hikmalayer Signed Message:\\n" + len(msg) + msg)
    signature = hex(compact ECDSA r||s), low-S normalised
"""

import hashlib
import re
from typing import Optional

from ecdsa import SigningKey, VerifyingKey, SECP256k1
from ecdsa.util import sigencode_string, sigdecode_string

ADDRESS_PREFIX = "hkm"
TOKEN_PREFIX = "hkt"
MESSAGE_PREFIX = b"\x19Hikmalayer Signed Message:\n"

# secp256k1 curve order — used for low-S normalisation
_CURVE_ORDER = SECP256k1.order
_HALF_ORDER = _CURVE_ORDER // 2

_ADDRESS_RE = re.compile(r"^hkm[0-9a-f]{40}$")
_TOKEN_RE = re.compile(r"^hkt[0-9a-f]{40}$")


def normalise_hex(value: str) -> str:
    """Strip an optional 0x prefix and lowercase."""
    return str(value or "").strip().removeprefix("0x").removeprefix("0X").lower()


def is_valid_address(address: str) -> bool:
    """True if the string is a well-formed hkm address."""
    return bool(_ADDRESS_RE.match(str(address or "")))


def is_valid_token_id(token_id: str) -> bool:
    """True if the string is a well-formed hkt token id."""
    return bool(_TOKEN_RE.match(str(token_id or "")))


def derive_public_key(private_key_hex: str) -> str:
    """
    Derive the uncompressed (65-byte, 0x04-prefixed) public key — the form
    the chain hashes for addresses and uses for signature verification.
    """
    sk = SigningKey.from_string(bytes.fromhex(normalise_hex(private_key_hex)),
                                curve=SECP256k1)
    vk = sk.get_verifying_key()
    return "04" + vk.to_string().hex()


def derive_address(public_key_hex: str) -> str:
    """hkm + first 20 bytes of SHA-256 over the uncompressed public key."""
    pub = bytes.fromhex(normalise_hex(public_key_hex))
    digest = hashlib.sha256(pub).digest()
    return ADDRESS_PREFIX + digest[:20].hex()


def message_digest(message: str) -> bytes:
    """
    The digest the chain signs: SHA-256 over the prefixed message.
    Matches src/consensus/pos.rs.
    """
    msg = message.encode("utf-8")
    prefix = MESSAGE_PREFIX + str(len(msg)).encode("ascii")
    return hashlib.sha256(prefix + msg).digest()


def _normalise_low_s(signature: bytes) -> bytes:
    """
    Enforce the low-S form. Without this, a valid signature has two
    encodings and the chain would reject one of them.
    """
    r = int.from_bytes(signature[:32], "big")
    s = int.from_bytes(signature[32:], "big")
    if s > _HALF_ORDER:
        s = _CURVE_ORDER - s
    return r.to_bytes(32, "big") + s.to_bytes(32, "big")


def sign_message(message: str, private_key_hex: str) -> str:
    """Sign a canonical message string. Returns 128 hex chars (64 bytes)."""
    sk = SigningKey.from_string(bytes.fromhex(normalise_hex(private_key_hex)),
                                curve=SECP256k1)
    digest = message_digest(message)
    raw = sk.sign_digest_deterministic(
        digest, hashfunc=hashlib.sha256, sigencode=sigencode_string
    )
    return _normalise_low_s(raw).hex()


def sign_digest(digest_hex: str, private_key_hex: str) -> str:
    """
    Sign a raw 32-byte digest — used for block signing, where the digest
    is the block hash itself rather than a prefixed message.
    """
    sk = SigningKey.from_string(bytes.fromhex(normalise_hex(private_key_hex)),
                                curve=SECP256k1)
    raw = sk.sign_digest_deterministic(
        bytes.fromhex(normalise_hex(digest_hex)),
        hashfunc=hashlib.sha256,
        sigencode=sigencode_string,
    )
    return _normalise_low_s(raw).hex()


def verify_message(message: str, signature_hex: str, public_key_hex: str) -> bool:
    """Verify a signature against a message and public key."""
    try:
        pub = normalise_hex(public_key_hex)
        if pub.startswith("04"):
            pub = pub[2:]
        vk = VerifyingKey.from_string(bytes.fromhex(pub), curve=SECP256k1)
        return vk.verify_digest(
            bytes.fromhex(normalise_hex(signature_hex)),
            message_digest(message),
            sigdecode=sigdecode_string,
        )
    except Exception:
        return False


class LocalSigner:
    """
    A key pair held in this process, with signing helpers for every
    transaction type.

        signer = LocalSigner.random()
        signer = LocalSigner.from_private_key("…64 hex chars…")

        print(signer.address)     # hkm…
        print(signer.public_key)  # 04…
    """

    def __init__(self, private_key_hex: str):
        self.private_key = normalise_hex(private_key_hex)
        if len(self.private_key) != 64:
            raise ValueError("A private key is 32 bytes (64 hex characters)")
        self.public_key = derive_public_key(self.private_key)
        self.address = derive_address(self.public_key)

    @classmethod
    def random(cls) -> "LocalSigner":
        """Generate a new key pair from the platform CSPRNG."""
        sk = SigningKey.generate(curve=SECP256k1)
        return cls(sk.to_string().hex())

    @classmethod
    def from_private_key(cls, private_key_hex: str) -> "LocalSigner":
        return cls(private_key_hex)

    def sign(self, message: str) -> str:
        """Sign an already-scoped canonical message."""
        return sign_message(message, self.private_key)

    def sign_block_hash(self, block_hash_hex: str) -> str:
        """Sign a proposed block hash (remote signer / HSM workflow)."""
        return sign_digest(block_hash_hex, self.private_key)

    def __repr__(self) -> str:  # never leak the private key
        return f"<LocalSigner {self.address}>"
