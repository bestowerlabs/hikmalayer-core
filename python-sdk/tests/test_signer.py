"""Keys, addresses and native signing."""

import pytest

from hikmalayer import (
    LocalSigner, derive_address, derive_public_key,
    is_valid_address, is_valid_token_id,
    sign_message, verify_message, ADDRESS_PREFIX,
)

# A fixed key, so these tests assert on stable values.
TEST_KEY = "80f91adc283392febbfc86b7327c055b8559373459040e07e78640e3ac592517"


class TestKeyGeneration:
    def test_random_produces_a_valid_address(self):
        signer = LocalSigner.random()
        assert is_valid_address(signer.address)

    def test_random_produces_an_uncompressed_public_key(self):
        signer = LocalSigner.random()
        assert signer.public_key.startswith("04")
        assert len(signer.public_key) == 130

    def test_each_key_is_distinct(self):
        a, b = LocalSigner.random(), LocalSigner.random()
        assert a.private_key != b.private_key
        assert a.address != b.address

    def test_import_round_trips(self):
        original = LocalSigner.random()
        imported = LocalSigner.from_private_key(original.private_key)
        assert imported.address == original.address
        assert imported.public_key == original.public_key

    def test_rejects_a_short_key(self):
        with pytest.raises(ValueError):
            LocalSigner("abcd")

    def test_accepts_a_0x_prefix(self):
        with_prefix = LocalSigner("0x" + TEST_KEY)
        without = LocalSigner(TEST_KEY)
        assert with_prefix.address == without.address

    def test_repr_does_not_leak_the_key(self):
        signer = LocalSigner(TEST_KEY)
        assert TEST_KEY not in repr(signer)
        assert signer.address in repr(signer)


class TestDerivation:
    def test_address_is_deterministic(self):
        pub = derive_public_key(TEST_KEY)
        assert derive_address(pub) == derive_address(pub)

    def test_address_shape(self):
        address = derive_address(derive_public_key(TEST_KEY))
        assert address.startswith(ADDRESS_PREFIX)
        assert len(address) == 43


class TestValidation:
    def test_accepts_a_real_address(self):
        assert is_valid_address(LocalSigner.random().address)

    @pytest.mark.parametrize("bad", [
        "", "hkm", "hkm-typo", "0x1234", None,
        "hkm50929b74c1a04954b78b4b6035e97a5e078a5a0",   # one short
        "hkm50929b74c1a04954b78b4b6035e97a5e078a5a0ff", # one long
        "HKM50929B74C1A04954B78B4B6035E97A5E078A5A0F",  # uppercase
    ])
    def test_rejects_malformed(self, bad):
        assert not is_valid_address(bad)

    def test_token_ids_use_their_own_prefix(self):
        assert is_valid_token_id("hkt" + "a" * 40)
        assert not is_valid_token_id("hkm" + "a" * 40)


class TestSigning:
    def test_signature_shape(self):
        sig = sign_message("hikmalayer-devnet:hikmalayer-transfer:a:b:1:0", TEST_KEY)
        assert len(sig) == 128
        assert all(c in "0123456789abcdef" for c in sig)

    def test_deterministic(self):
        msg = "hikmalayer-devnet:hikmalayer-transfer:a:b:1000000:5"
        assert sign_message(msg, TEST_KEY) == sign_message(msg, TEST_KEY)

    def test_different_message_different_signature(self):
        a = sign_message("hikmalayer-devnet:x:1", TEST_KEY)
        b = sign_message("hikmalayer-devnet:x:2", TEST_KEY)
        assert a != b

    def test_verifies_against_the_public_key(self):
        signer = LocalSigner(TEST_KEY)
        msg = "hikmalayer-devnet:hikmalayer-transfer:a:b:1000000:5"
        sig = signer.sign(msg)
        assert verify_message(msg, sig, signer.public_key)

    def test_a_tampered_message_does_not_verify(self):
        signer = LocalSigner(TEST_KEY)
        sig = signer.sign("hikmalayer-devnet:amount:100")
        assert not verify_message("hikmalayer-devnet:amount:999", sig,
                                  signer.public_key)

    def test_another_key_does_not_verify(self):
        msg = "hikmalayer-devnet:x:1"
        sig = LocalSigner(TEST_KEY).sign(msg)
        assert not verify_message(msg, sig, LocalSigner.random().public_key)
