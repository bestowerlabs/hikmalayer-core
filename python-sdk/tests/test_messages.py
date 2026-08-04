"""
Canonical signing domains.

These strings are consensus. If one changes, every previously signed
transaction of that type stops verifying — so they are asserted literally.
"""

from hikmalayer import messages

A = "hkm13320761030a4c59d96060708e2377bc4e936dee"
B = "hkm0000000000000000000000000000000000000001"
T = "hktcc3f73fed737c988826bc2540f1483bf8a640993"


def test_transfer():
    assert messages.transfer(A, B, 1_500_000, 7) == \
        f"hikmalayer-transfer:{A}:{B}:1500000:7"


def test_stake_without_vrf():
    assert messages.stake(A, 10_000_000_000, 3) == \
        f"hikmalayer-stake:{A}:10000000000:3"


def test_stake_binds_the_vrf_key():
    vrf = "927afe8bb94d5c5170b730487aa9e431ceb1270cdb74a0d547badf54ff6a3370"
    assert messages.stake(A, 10_000_000_000, 3, vrf) == \
        f"hikmalayer-stake:{A}:10000000000:3:{vrf}"


def test_withdraw():
    assert messages.withdraw(A, 5_000_000, 4) == \
        f"hikmalayer-withdraw:{A}:5000000:4"


def test_vest():
    assert messages.vest(A, B, 2_000_000, 100, 1000, 5) == \
        f"hikmalayer-vest:{A}:{B}:2000000:100:1000:5"


def test_token_create():
    assert messages.token_create("TESTX", "Test Asset", 6, 1_000_000_000_000, 2) == \
        "hikmalayer-token-create:TESTX:Test Asset:6:1000000000000:2"


def test_token_create_with_non_ascii():
    assert messages.token_create("CAFÉ", "Café 日本 ☕", 8, 42, 1) == \
        "hikmalayer-token-create:CAFÉ:Café 日本 ☕:8:42:1"


def test_token_transfer():
    # No sender in the domain — the signing key establishes it.
    # Verified against `hikma-wallet sign-token-transfer`.
    assert messages.token_transfer(T, B, 250_000, 9) == \
        f"hikmalayer-token-transfer:{T}:{B}:250000:9"


def test_token_burn():
    # Verified against `hikma-wallet sign-token-burn`.
    assert messages.token_burn(T, 125_000, 10) == \
        f"hikmalayer-token-burn:{T}:125000:10"


def test_amm_add():
    assert messages.amm_add(T, 100_000_000_000, 500_000_000_000,
                            222_488_762_765, 11) == \
        f"hikmalayer-amm-add:{T}:100000000000:500000000000:222488762765:11"


def test_amm_remove():
    assert messages.amm_remove(T, 100_000_000_000, 44_497_752_752,
                               222_488_763_761, 12) == \
        f"hikmalayer-amm-remove:{T}:100000000000:44497752752:222488763761:12"


def test_amm_swap_hkm_to_token():
    assert messages.amm_swap(T, True, 100_000_000, 480_000_000, 13) == \
        f"hikmalayer-amm-swap:{T}:true:100000000:480000000:13"


def test_amm_swap_token_to_hkm():
    assert messages.amm_swap(T, False, 500_000_000, 95_000_000, 14) == \
        f"hikmalayer-amm-swap:{T}:false:500000000:95000000:14"


def test_credential():
    assert messages.credential("cert-1", "hkm-subject", "deadbeef", False, 15) == \
        "hikmalayer-credential:cert-1:hkm-subject:deadbeef:false:15"
    assert messages.credential("cert-1", "hkm-subject", "deadbeef", True, 16) == \
        "hikmalayer-credential:cert-1:hkm-subject:deadbeef:true:16"


def test_amounts_beyond_2_53_are_exact():
    big = 2**53 + 1
    assert str(big) in messages.transfer(A, B, big, 8)


def test_network_scoping():
    domain = messages.transfer(A, B, 1, 0)
    assert messages.scoped("hikmalayer-mainnet", domain) == \
        f"hikmalayer-mainnet:{domain}"


def test_the_same_transaction_signs_differently_per_network():
    domain = messages.transfer(A, B, 1, 0)
    assert messages.scoped("hikmalayer-devnet", domain) != \
        messages.scoped("hikmalayer-mainnet", domain)
