"""
Hybrid (quantum-ready) accounts.

The parity tests use fixtures produced by `hikma-wallet identity`. If any
of them fails, this SDK and the chain disagree about what a hybrid account
is, and every hkq address it derives is wrong — so they assert the exact
values rather than the shape.
"""

import pytest

from hikmalayer import hybrid
from hikmalayer.hybrid import (
    HYBRID_ADDRESS_PREFIX,
    PQ_PUBLIC_KEY_LEN,
    PQ_SIGNATURE_LEN,
    HybridSigningUnavailable,
    derive_hybrid_address,
    derive_hybrid_identity,
    derive_pq_public_key,
    derive_pq_seed,
    is_canonical_pq_public_key,
    is_canonical_public_key,
    is_hybrid_address,
    is_valid_pq_public_key,
    pq_available,
)

# Skip the whole module rather than fail it where ML-DSA is unavailable:
# an old `cryptography` is a missing capability, not a broken SDK.
pytestmark = pytest.mark.skipif(
    not pq_available(),
    reason="needs cryptography>=46 for ML-DSA",
)


# ── fixtures from hikma-wallet ───────────────────────────────────────────

ISSUER_KEY = "b670e27c4d293e55327f07f04d83bfa514dc726aa2a8c4e44a3fd45b49b39484"
ISSUER_PUB = (
    "0474d194d724277c8760659a10b7e7d138669811e9ab43c0f070ed757bd7c786"
    "9f8352ec51ba438beafe11e71fdd3bb5fe5f8a503dd4244fbb1f402ebabb82ddd5"
)
ISSUER_HKQ = "hkq42ed00179def1e6eb2fa816c1864810cd8cf33b3"
ISSUER_PQ_PREFIX = "94397517c8b07080fd3cd089bcbfcb9f69ba92354a386e990ca951ba20aa1557"

STUDENT_KEY = "dc56c4394db5f2a28333d86b3e0724a1edffb292dbd641dc528310462adc6a62"
STUDENT_PUB = (
    "04fb69de82aaf1337cd69a9848e7f61e15269166b19b7fa6bf2aec2e612f00be"
    "d47f683f749c2de22f422803b8937be7a9770ffea4f35983beb35bfd513a84ce3d"
)
STUDENT_HKQ = "hkq1bba22f251ed3aff86b59e2f4268bd6ef8ab16e5"


class TestParityWithCLI:
    """Byte-for-byte agreement with `hikma-wallet identity`."""

    def test_issuer_address(self):
        assert derive_hybrid_identity(ISSUER_KEY)["address"] == ISSUER_HKQ

    def test_student_address(self):
        assert derive_hybrid_identity(STUDENT_KEY)["address"] == STUDENT_HKQ

    def test_classical_public_key(self):
        assert derive_hybrid_identity(ISSUER_KEY)["public_key"] == ISSUER_PUB

    def test_pq_public_key(self):
        pq = derive_pq_public_key(ISSUER_KEY)
        assert pq.startswith(ISSUER_PQ_PREFIX)
        assert len(bytes.fromhex(pq)) == PQ_PUBLIC_KEY_LEN

    def test_one_secret_two_identities(self):
        # The point of the design: the hybrid key is derived, not a second
        # thing to back up.
        ident = derive_hybrid_identity(ISSUER_KEY)
        assert ident["public_key"] == ISSUER_PUB
        assert ident["address"].startswith(HYBRID_ADDRESS_PREFIX)


class TestSeedDerivation:
    def test_deterministic(self):
        assert derive_pq_seed(ISSUER_KEY) == derive_pq_seed(ISSUER_KEY)

    def test_thirty_two_bytes(self):
        assert len(derive_pq_seed(ISSUER_KEY)) == 32

    def test_distinct_keys_distinct_seeds(self):
        assert derive_pq_seed(ISSUER_KEY) != derive_pq_seed(STUDENT_KEY)

    def test_domain_separated_from_the_master_secret(self):
        # The seed must not be the private key itself, or one break would
        # give the attacker both schemes.
        assert derive_pq_seed(ISSUER_KEY).hex() != ISSUER_KEY

    @pytest.mark.parametrize("bad", ["", "abcd", "zz" * 32, "00" * 31])
    def test_rejects_malformed_keys(self, bad):
        with pytest.raises(ValueError):
            derive_pq_seed(bad)


class TestAddressDerivation:
    def test_commits_to_both_keys(self):
        # Substituting either key must name a different account — this is
        # what stops a broken-secp256k1 attacker reusing the victim's
        # classical key with an ML-DSA key of their own.
        issuer_pq = derive_pq_public_key(ISSUER_KEY)
        student_pq = derive_pq_public_key(STUDENT_KEY)

        real = derive_hybrid_address(ISSUER_PUB, issuer_pq)
        swapped_pq = derive_hybrid_address(ISSUER_PUB, student_pq)
        swapped_classical = derive_hybrid_address(STUDENT_PUB, issuer_pq)

        assert real != swapped_pq
        assert real != swapped_classical
        assert swapped_pq != swapped_classical

    def test_shape(self):
        address = derive_hybrid_identity(ISSUER_KEY)["address"]
        assert address.startswith("hkq")
        assert len(address) == 43
        assert is_hybrid_address(address)

    def test_differs_from_the_classical_address(self):
        # Same secret, two accounts. Sending to the wrong one would be
        # unrecoverable, so they must never collide.
        from hikmalayer.signer import derive_address
        assert derive_hybrid_identity(ISSUER_KEY)["address"] != \
            derive_address(ISSUER_PUB)

    def test_rejects_compressed_public_key(self):
        # A compressed key hashes to the same account but is a second
        # on-wire spelling, which would give one transaction two ids.
        compressed = "02" + ISSUER_PUB[2:66]
        with pytest.raises(ValueError, match="canonical"):
            derive_hybrid_address(compressed, derive_pq_public_key(ISSUER_KEY))

    def test_rejects_uppercase_public_key(self):
        with pytest.raises(ValueError, match="canonical"):
            derive_hybrid_address(ISSUER_PUB.upper(),
                                  derive_pq_public_key(ISSUER_KEY))

    def test_rejects_a_point_not_on_the_curve(self):
        # Well-formed hex, but not a real key. If this reached the hash it
        # would produce an address nobody could ever sign for.
        off_curve = "04" + "11" * 64
        with pytest.raises(ValueError):
            derive_hybrid_address(off_curve, derive_pq_public_key(ISSUER_KEY))

    def test_rejects_wrong_length_pq_key(self):
        with pytest.raises(ValueError, match="1952"):
            derive_hybrid_address(ISSUER_PUB, "ab" * 100)


class TestValidation:
    def test_accepts_canonical_public_key(self):
        assert is_canonical_public_key(ISSUER_PUB)

    @pytest.mark.parametrize("bad", [
        "", None, "04", ISSUER_PUB.upper(),
        "02" + ISSUER_PUB[2:66],          # compressed
        ISSUER_PUB[:-2],                   # truncated
        "04" + "11" * 64,                  # not on the curve
    ])
    def test_rejects_non_canonical(self, bad):
        assert not is_canonical_public_key(bad)

    def test_pq_key_length(self):
        pq = derive_pq_public_key(ISSUER_KEY)
        assert is_valid_pq_public_key(pq)
        assert is_canonical_pq_public_key(pq)
        assert not is_valid_pq_public_key(pq[:-2])

    def test_pq_key_must_be_lowercase(self):
        assert not is_canonical_pq_public_key(derive_pq_public_key(ISSUER_KEY).upper())

    @pytest.mark.parametrize("bad", [
        "", None, "hkq", "hkm" + "a" * 40,
        "hkq" + "a" * 39, "hkq" + "a" * 41,
        ("hkq" + "a" * 40).upper(),
    ])
    def test_rejects_malformed_hybrid_addresses(self, bad):
        assert not is_hybrid_address(bad)


class TestSigningIsUnavailable:
    """
    Signing must fail loudly and say why. Returning a hedged signature
    would be valid on chain but not byte-identical to the CLI, and Python
    would become the one client whose signatures depend on the local RNG.
    """

    def test_pq_sign_raises(self):
        with pytest.raises(HybridSigningUnavailable):
            hybrid.pq_sign_message("anything", ISSUER_KEY)

    def test_sign_hybrid_raises(self):
        with pytest.raises(HybridSigningUnavailable):
            hybrid.sign_hybrid("anything", ISSUER_KEY)

    def test_the_error_says_where_to_sign_instead(self):
        with pytest.raises(HybridSigningUnavailable) as excinfo:
            hybrid.pq_sign_message("anything", ISSUER_KEY)
        text = str(excinfo.value)
        assert "JavaScript SDK" in text or "hikma-wallet" in text

    def test_it_is_a_notimplementederror(self):
        # So `except NotImplementedError` catches it without importing
        # anything from this module.
        assert issubclass(HybridSigningUnavailable, NotImplementedError)


class TestConstants:
    def test_ml_dsa_65_sizes(self):
        assert PQ_PUBLIC_KEY_LEN == 1952
        assert PQ_SIGNATURE_LEN == 3309

    def test_domain_separators_are_distinct(self):
        # Reusing one separator for two purposes is the mistake they exist
        # to prevent.
        separators = {
            hybrid.PQ_SEED_DOMAIN,
            hybrid.PQ_SIGN_DOMAIN,
            hybrid.HYBRID_ADDRESS_DOMAIN,
            hybrid.PQ_CONTEXT,
        }
        assert len(separators) == 4
