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

fn kept(password: Option<&str>) -> KeptPairing {
    KeptPairing { keys: keys(), password: password.map(String::from) }
}

#[test]
fn kept_text_reads_back_as_the_same_pairing() {
    for pairing in [kept(None), kept(Some("kitchen pässword"))] {
        assert_eq!(KeptPairing::decode(&pairing.encode()), Ok(pairing));
    }
}

#[test]
fn malformed_kept_text_fails_and_names_what_is_wrong() {
    let text = kept(None).encode();
    let fields: Vec<&str> = text.split(':').collect();
    let with = |index: usize, value: &str| {
        let mut changed = fields.clone();
        changed[index] = value;
        changed.join(":")
    };

    assert_eq!(KeptPairing::decode(""), Err(MalformedPairingKeys::FieldCount(1)));
    assert_eq!(
        KeptPairing::decode(&format!("{text}:00")),
        Err(MalformedPairingKeys::FieldCount(6))
    );
    assert!(matches!(
        KeptPairing::decode(&with(1, "zz")),
        Err(MalformedPairingKeys::Hex { field: "seed", .. })
    ));
    assert_eq!(
        KeptPairing::decode(&with(1, "1111")),
        Err(MalformedPairingKeys::Length { field: "seed", len: 2 })
    );
    assert_eq!(
        KeptPairing::decode(&with(3, "22")),
        Err(MalformedPairingKeys::Length { field: "receiver public key", len: 1 })
    );
    assert!(matches!(
        KeptPairing::decode(&with(0, "ff")),
        Err(MalformedPairingKeys::ControllerId(_))
    ));
    assert_eq!(KeptPairing::decode(&with(2, "")), Err(MalformedPairingKeys::EmptyIdentifier));
    assert!(matches!(KeptPairing::decode(&with(4, "ff")), Err(MalformedPairingKeys::Password(_))));
}

/// Debug output never carries the seed or the password.
#[test]
fn debug_output_leaves_out_the_seed_and_the_password() {
    let debug = format!("{:?}", kept(Some("hunter2")));
    assert!(!debug.contains(&hex::encode([0x11u8; 32])), "{debug}");
    assert!(!debug.contains("17, 17"), "{debug}");
    assert!(!debug.contains("hunter2"), "{debug}");
    assert!(debug.contains("6F1C2A4E"), "{debug}");
}
