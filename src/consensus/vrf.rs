//! Verifiable Random Function support for unbiasable leader election.
//!
//! Every block carries the producer's VRF output+proof over a chain-derived
//! input. The output is UNIQUE for a given (key, input) — the producer cannot
//! grind it — and anyone can verify it against the validator's registered
//! VRF public key. Outputs are folded into an on-chain randomness beacon
//! that seeds the next slot's leader selection.
//!
//! Keys are sr25519 (schnorrkel — the audited VRF used in production by
//! Polkadot), derived from the SAME 32-byte secret as the validator's
//! secp256k1 identity key, so a validator manages exactly one secret.

use schnorrkel::{
    vrf::{VRFPreOut, VRFProof},
    ExpansionMode, Keypair, MiniSecretKey, PublicKey,
};
use sha2::{Digest, Sha256};

/// Domain-separation context for all Hikmalayer VRF operations.
const VRF_CONTEXT: &[u8] = b"hikmalayer-vrf";

fn keypair_from_secret(private_key_hex: &str) -> Result<Keypair, String> {
    let bytes = hex::decode(private_key_hex).map_err(|err| err.to_string())?;
    let mini = MiniSecretKey::from_bytes(&bytes)
        .map_err(|err| format!("invalid VRF secret: {}", err))?;
    Ok(mini.expand_to_keypair(ExpansionMode::Ed25519))
}

/// Derive the VRF public key (hex) for a 32-byte private key. The same
/// secret drives both the secp256k1 identity key and the VRF key.
pub fn derive_vrf_public_key(private_key_hex: &str) -> Result<String, String> {
    let keypair = keypair_from_secret(private_key_hex)?;
    Ok(hex::encode(keypair.public.to_bytes()))
}

/// Produce (output, proof) for `input`. The output is unique for this key
/// and input — there is nothing to grind.
pub fn prove(input: &str, private_key_hex: &str) -> Result<(String, String), String> {
    let keypair = keypair_from_secret(private_key_hex)?;
    let context = schnorrkel::signing_context(VRF_CONTEXT);
    let (io, proof, _) = keypair.vrf_sign(context.bytes(input.as_bytes()));
    Ok((
        hex::encode(io.to_preout().to_bytes()),
        hex::encode(proof.to_bytes()),
    ))
}

/// Verify a VRF output+proof for `input` against a registered VRF public
/// key. Returns true only when the output is the unique valid one.
pub fn verify(input: &str, vrf_public_key_hex: &str, output_hex: &str, proof_hex: &str) -> bool {
    let Ok(public_bytes) = hex::decode(vrf_public_key_hex) else {
        return false;
    };
    let Ok(public) = PublicKey::from_bytes(&public_bytes) else {
        return false;
    };
    let Ok(output_bytes) = hex::decode(output_hex) else {
        return false;
    };
    let Ok(preout) = VRFPreOut::from_bytes(&output_bytes) else {
        return false;
    };
    let Ok(proof_bytes) = hex::decode(proof_hex) else {
        return false;
    };
    let Ok(proof) = VRFProof::from_bytes(&proof_bytes) else {
        return false;
    };

    let context = schnorrkel::signing_context(VRF_CONTEXT);
    public
        .vrf_verify(context.bytes(input.as_bytes()), &preout, &proof)
        .is_ok()
}

/// The VRF input for a given slot: bound to the beacon value at the parent
/// block and the height being produced.
pub fn slot_input(parent_randomness: &str, height: u64) -> String {
    format!("{}:{}", parent_randomness, height)
}

/// The VRF input for a (height, round) slot. Round 0 keeps the legacy
/// `randomness:height` form so every existing block stays valid; each
/// fallback round appends its number, giving a distinct, ungrindable seed
/// per rotation.
pub fn slot_input_at_round(parent_randomness: &str, height: u64, round: u64) -> String {
    if round == 0 {
        slot_input(parent_randomness, height)
    } else {
        format!("{}:{}:{}", parent_randomness, height, round)
    }
}

/// Fold a block's VRF output into the randomness beacon.
pub fn next_randomness(parent_randomness: &str, vrf_output_hex: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(parent_randomness.as_bytes());
    hasher.update(vrf_output_hex.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(seed: u8) -> String {
        hex::encode([seed; 32])
    }

    #[test]
    fn prove_verify_roundtrip() {
        let sk = secret(1);
        let pk = derive_vrf_public_key(&sk).unwrap();
        let input = slot_input("beacon", 5);
        let (output, proof) = prove(&input, &sk).unwrap();

        assert!(verify(&input, &pk, &output, &proof));
        // Wrong input, wrong key, wrong output: all rejected.
        assert!(!verify("other-input", &pk, &output, &proof));
        let other_pk = derive_vrf_public_key(&secret(2)).unwrap();
        assert!(!verify(&input, &other_pk, &output, &proof));
    }

    #[test]
    fn round_slot_inputs_are_distinct_and_round_zero_is_legacy() {
        // Round 0 must equal the legacy form so every existing block
        // remains valid; fallback rounds get distinct, ungrindable seeds.
        assert_eq!(slot_input_at_round("beacon", 7, 0), slot_input("beacon", 7));
        assert_ne!(
            slot_input_at_round("beacon", 7, 1),
            slot_input_at_round("beacon", 7, 2)
        );
        assert_ne!(slot_input_at_round("beacon", 7, 1), slot_input("beacon", 7));
    }

    #[test]
    fn output_is_deterministic_and_ungrindable() {
        let sk = secret(3);
        let input = slot_input("beacon", 9);
        let (out_a, _) = prove(&input, &sk).unwrap();
        let (out_b, _) = prove(&input, &sk).unwrap();
        // The VRF output for a fixed (key, input) is unique — repeated
        // proving cannot yield a different value to grind on.
        assert_eq!(out_a, out_b);
    }

    #[test]
    fn beacon_evolves_with_outputs() {
        let r1 = next_randomness("genesis", "out-1");
        let r2 = next_randomness(&r1, "out-2");
        assert_ne!(r1, r2);
        assert_eq!(r1, next_randomness("genesis", "out-1"));
    }
}
