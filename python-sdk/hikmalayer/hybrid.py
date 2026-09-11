"""
Hybrid (quantum-ready) accounts.

Mirrors `sdk/src/hybrid.js` and `src/consensus/pq.rs`. A hybrid account is
authorised by TWO signatures over the same message — secp256k1 ECDSA and
ML-DSA-65 (FIPS 204) — and both must verify. An attacker therefore has to
break both schemes to forge one transaction.

    hkm…  classical  address = SHA256(secp_pub)[:20]
    hkq…  hybrid     address = SHA256(domain ‖ secp_pub ‖ mldsa_pub)[:20]

The hybrid address commits to BOTH public keys. That is what stops an
attacker who broke secp256k1 from presenting the victim's classical key
alongside an ML-DSA key of their own: substituting either key names a
different account.


WHAT THIS MODULE DOES NOT DO
----------------------------
It derives hybrid addresses and verifies hybrid signatures. It does not
sign for them.

Signing requires FIPS 204's `rnd` parameter to be supplied by the caller.
The chain pins it to SHA256(domain ‖ seed ‖ message) so that signatures are
reproducible and a broken RNG on the user's machine cannot leak the key —
`fips204::try_sign_with_seed` in Rust, `extraEntropy` in @noble/post-quantum.

No Python ML-DSA implementation exposes that parameter:

  * pyca/cryptography      sign(data, context) and sign_mu(mu) — no rnd
  * dilithium-py           author states it is educational and not
                           constant-time, so unsuitable for secret material
  * pqcrypto               0.4.0 accepted a tampered ML-DSA-65 message as
                           valid; a signature library cannot have that bug
  * liboqs-python          upstream documents it as prototypical

Signing with hedged randomness would produce valid signatures the chain
accepts, but they would not be byte-identical to the CLI, and Python would
be the one client whose signatures depend on the local RNG. That asymmetry
is invisible in the API and only shows up when someone's entropy is poor,
so this module does not offer it.

To sign for a hybrid account, use the JavaScript SDK or `hikma-wallet`.
"""

import hashlib
import re
from typing import Optional

from ecdsa import SECP256k1, VerifyingKey
from ecdsa.ellipticcurve import Point

from .signer import derive_public_key, normalise_hex

# ── constants ────────────────────────────────────────────────────────────

HYBRID_ADDRESS_PREFIX = "hkq"

# ML-DSA-65 sizes, in bytes. Honest cost: ~5.3 KB of key and signature
# material per hybrid transaction, against ~130 bytes classical.
PQ_PUBLIC_KEY_LEN = 1952
PQ_SIGNATURE_LEN = 3309

# Domain separators. Each keeps one set of 32 bytes from keying two
# different things — the reuse that turns one break into two.
PQ_SEED_DOMAIN = b"hikmalayer-pq-key-v1"
PQ_SIGN_DOMAIN = b"hikmalayer-pq-sign-v1"
HYBRID_ADDRESS_DOMAIN = b"hikmalayer-hybrid-address-v1"

# FIPS 204's application context string. Names the chain, so an ML-DSA
# signature made for Hikmalayer cannot be lifted into another protocol.
PQ_CONTEXT = b"hikmalayer"

_HYBRID_ADDRESS_RE = re.compile(r"^hkq[0-9a-f]{40}$")
_CANONICAL_PUB_RE = re.compile(r"^04[0-9a-f]{128}$")
_LOWER_HEX_RE = re.compile(r"^[0-9a-f]+$")


class HybridSigningUnavailable(NotImplementedError):
    """
    Raised when hybrid signing is attempted from Python.

    Carries the reason rather than failing opaquely — see the module
    docstring for why no Python ML-DSA implementation is suitable.
    """

    def __init__(self, message: Optional[str] = None):
        super().__init__(message or (
            "Hybrid signing is not available in the Python SDK. Signing a "
            "hybrid (hkq) account needs FIPS 204's rnd parameter, which the "
            "chain pins to a derived value so signatures do not depend on "
            "the local RNG. No Python ML-DSA library exposes it. Use the "
            "JavaScript SDK or `hikma-wallet` to sign; this SDK derives "
            "hybrid addresses and verifies hybrid signatures."
        ))


# ── ML-DSA availability ──────────────────────────────────────────────────

def _mldsa():
    """
    The ML-DSA-65 implementation, or None.

    pyca/cryptography gained ML-DSA in version 46. Verification needs no
    secret material, so its lack of an rnd parameter does not matter here.
    """
    try:
        from cryptography.hazmat.primitives.asymmetric import mldsa
        return mldsa
    except ImportError:
        return None


def pq_available() -> bool:
    """
    True if this environment can derive and verify post-quantum keys.

    Callers that must handle hkq accounts should check this at startup
    rather than discovering it at the first verification.
    """
    return _mldsa() is not None


def _require_mldsa():
    mldsa = _mldsa()
    if mldsa is None:
        raise RuntimeError(
            "Post-quantum support needs `cryptography` 46 or later with "
            "ML-DSA. Install it with: pip install 'cryptography>=46'"
        )
    return mldsa


# ── seed and key derivation ──────────────────────────────────────────────

def derive_pq_seed(private_key_hex: str) -> bytes:
    """
    The ML-DSA seed (FIPS 204's xi) for an account.

    Derived from the master secret rather than stored separately: one
    backup still covers both schemes.
    """
    key = normalise_hex(private_key_hex)
    raw = bytes.fromhex(key)
    if len(raw) != 32:
        raise ValueError("Not a valid 32-byte secp256k1 private key")
    return hashlib.sha256(PQ_SEED_DOMAIN + raw).digest()


def derive_pq_public_key(private_key_hex: str) -> str:
    """The account's ML-DSA-65 public key, as lower-case hex."""
    mldsa = _require_mldsa()
    seed = derive_pq_seed(private_key_hex)
    private = mldsa.MLDSA65PrivateKey.from_seed_bytes(seed)
    return private.public_key().public_bytes_raw().hex()


# ── validation ───────────────────────────────────────────────────────────

def is_valid_pq_public_key(value: str) -> bool:
    """True if the value is 1952 bytes of hex."""
    try:
        return len(bytes.fromhex(normalise_hex(value))) == PQ_PUBLIC_KEY_LEN
    except (ValueError, TypeError):
        return False


def is_canonical_public_key(value: str) -> bool:
    """
    A public key's ONE canonical spelling: uncompressed, 65 bytes,
    04-prefixed, lower-case hex.

    secp256k1 accepts a 33-byte compressed encoding of the same key, and
    hex accepts upper case. Both hash to the same address and verify the
    same signatures, so allowing them would give one authorised
    transaction more than one valid on-wire form — and therefore more than
    one transaction id. The node rejects them; mirrors
    `pos::canonical_public_key`.
    """
    text = str(value or "").strip()
    if not _CANONICAL_PUB_RE.match(text):
        return False
    try:
        # Reject a well-formed string that is not actually on the curve.
        VerifyingKey.from_string(bytes.fromhex(text[2:]), curve=SECP256k1)
        return True
    except Exception:
        return False


def is_canonical_pq_public_key(value: str) -> bool:
    """The same rule for an ML-DSA key: lower-case hex, exact length."""
    text = str(value or "").strip()
    return bool(_LOWER_HEX_RE.match(text)) and is_valid_pq_public_key(text)


def is_hybrid_address(value: str) -> bool:
    """True if the value is a well-formed hkq address."""
    return bool(_HYBRID_ADDRESS_RE.match(str(value or "").strip()))


# ── address derivation ───────────────────────────────────────────────────

def derive_hybrid_address(public_key_hex: str, pq_public_key_hex: str) -> str:
    """
    Derive a hybrid address from both public keys.

    Canonical forms only — and a malformed point is rejected on the way, so
    it can never reach the hash and become an address nobody could ever
    sign for.
    """
    public_key = str(public_key_hex or "").strip()
    pq_public_key = str(pq_public_key_hex or "").strip()

    if not is_canonical_public_key(public_key):
        raise ValueError(
            "classical public key must be the canonical uncompressed form "
            "(04 + 128 lower-case hex)"
        )
    if not is_canonical_pq_public_key(pq_public_key):
        raise ValueError(
            "post-quantum public key must be 1952 bytes of lower-case hex"
        )

    digest = hashlib.sha256(
        HYBRID_ADDRESS_DOMAIN
        + bytes.fromhex(public_key)
        + bytes.fromhex(pq_public_key)
    ).digest()
    return HYBRID_ADDRESS_PREFIX + digest[:20].hex()


def derive_hybrid_identity(private_key_hex: str) -> dict:
    """
    Everything a hybrid account is, from the one secret the user already
    has.

        {"address": "hkq…", "public_key": "04…", "pq_public_key": "…"}
    """
    public_key = derive_public_key(private_key_hex)
    pq_public_key = derive_pq_public_key(private_key_hex)
    return {
        "address": derive_hybrid_address(public_key, pq_public_key),
        "public_key": public_key,
        "pq_public_key": pq_public_key,
    }


# ── verification ─────────────────────────────────────────────────────────

def pq_verify_message(message: str, pq_public_key_hex: str,
                      pq_signature_hex: str) -> bool:
    """
    Verify an ML-DSA signature. Returns False for malformed input rather
    than raising, so a caller cannot mistake "could not check" for "valid".
    """
    mldsa = _mldsa()
    if mldsa is None:
        raise RuntimeError(
            "Post-quantum verification needs `cryptography` 46 or later. "
            "Refusing to return False, which would read as 'invalid "
            "signature' rather than 'could not check'."
        )
    try:
        public_key = bytes.fromhex(normalise_hex(pq_public_key_hex))
        signature = bytes.fromhex(normalise_hex(pq_signature_hex))
        if len(public_key) != PQ_PUBLIC_KEY_LEN:
            return False
        if len(signature) != PQ_SIGNATURE_LEN:
            return False

        verifier = mldsa.MLDSA65PublicKey.from_public_bytes(public_key)
        verifier.verify(signature, message.encode("utf-8"), context=PQ_CONTEXT)
        return True
    except Exception:
        return False


def verify_hybrid(message: str, address: str, public_key_hex: str,
                  signature_hex: str, pq_public_key_hex: str,
                  pq_signature_hex: str) -> bool:
    """
    Verify a hybrid transaction, in full. All three must hold:

      1. the address is the one both public keys derive to
      2. the ECDSA signature verifies
      3. the ML-DSA signature verifies

    Checking the address first is what makes the pair binding: without it,
    an attacker who broke secp256k1 could pair the victim's classical key
    with an ML-DSA key of their own and both signatures would verify.
    """
    from .signer import verify_message

    try:
        expected = derive_hybrid_address(public_key_hex, pq_public_key_hex)
    except ValueError:
        return False

    if expected != str(address or "").strip():
        return False
    if not verify_message(message, signature_hex, public_key_hex):
        return False
    return pq_verify_message(message, pq_public_key_hex, pq_signature_hex)


# ── signing: deliberately unavailable ────────────────────────────────────

def pq_sign_message(message: str, private_key_hex: str) -> str:
    """Not available in Python — see the module docstring."""
    raise HybridSigningUnavailable()


def sign_hybrid(message: str, private_key_hex: str) -> dict:
    """Not available in Python — see the module docstring."""
    raise HybridSigningUnavailable()
