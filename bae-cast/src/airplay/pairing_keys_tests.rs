use super::*;

fn keys() -> PairingKeys {
    PairingKeys {
        controller: ControllerIdentity {
            id: "6F1C2A4E-0B3D-4C5E-8F70-9A1B2C3D4E5F".to_string(),
            seed: [0x11; 32],
        },
        receiver_id: b"58:55:CA:1A:2B:3C".to_vec(),
        receiver_public: [0x22; 32],
    }
}

#[test]
fn kept_text_reads_back_as_the_same_keys() {
    let text = keys().encode();
    assert_eq!(PairingKeys::decode(&text), Ok(keys()));
}

#[test]
fn malformed_kept_text_fails_and_names_what_is_wrong() {
    let text = keys().encode();
    let fields: Vec<&str> = text.split(':').collect();
    let with = |index: usize, value: &str| {
        let mut changed = fields.clone();
        changed[index] = value;
        changed.join(":")
    };

    assert_eq!(PairingKeys::decode(""), Err(MalformedPairingKeys::FieldCount(1)));
    assert_eq!(
        PairingKeys::decode(&format!("{text}:00")),
        Err(MalformedPairingKeys::FieldCount(5))
    );
    assert!(matches!(
        PairingKeys::decode(&with(1, "zz")),
        Err(MalformedPairingKeys::Hex { field: "seed", .. })
    ));
    assert_eq!(
        PairingKeys::decode(&with(1, "1111")),
        Err(MalformedPairingKeys::Length { field: "seed", len: 2 })
    );
    assert_eq!(
        PairingKeys::decode(&with(3, "22")),
        Err(MalformedPairingKeys::Length { field: "receiver public key", len: 1 })
    );
    assert!(matches!(
        PairingKeys::decode(&with(0, "ff")),
        Err(MalformedPairingKeys::ControllerId(_))
    ));
    assert_eq!(PairingKeys::decode(&with(2, "")), Err(MalformedPairingKeys::EmptyIdentifier));
}

/// Debug output never carries the seed.
#[test]
fn debug_output_leaves_out_the_seed() {
    let debug = format!("{:?}", keys());
    assert!(!debug.contains(&hex::encode([0x11u8; 32])), "{debug}");
    assert!(!debug.contains("17, 17"), "{debug}");
    assert!(debug.contains("6F1C2A4E"), "{debug}");
}
