use super::*;

#[test]
fn round_trips_small_items() {
    let tlv = Tlv8::new()
        .push_u8(tlv_type::STATE, state::M1)
        .push_u8(tlv_type::METHOD, 0)
        .push(tlv_type::SALT, vec![1, 2, 3, 4]);
    let bytes = tlv.encode();
    // type,len,value triples.
    assert_eq!(bytes, vec![0x06, 1, 0x01, 0x00, 1, 0x00, 0x02, 4, 1, 2, 3, 4]);

    let back = Tlv8::decode(&bytes).unwrap();
    assert_eq!(back.get_u8(tlv_type::STATE), Some(state::M1));
    assert_eq!(back.get_u8(tlv_type::METHOD), Some(0));
    assert_eq!(back.get(tlv_type::SALT), Some([1, 2, 3, 4].as_slice()));
}

/// A 384-byte public key (SRP 3072-bit) is fragmented into 255 + 129 on the
/// wire and reassembled on decode.
#[test]
fn fragments_and_reassembles_a_long_value() {
    let key: Vec<u8> = (0..384).map(|i| (i % 256) as u8).collect();
    let tlv = Tlv8::new().push(tlv_type::PUBLIC_KEY, key.clone());
    let bytes = tlv.encode();

    // First chunk: type, 255, 255 bytes; second: type, 129, 129 bytes.
    assert_eq!(bytes[0], tlv_type::PUBLIC_KEY);
    assert_eq!(bytes[1], 255);
    assert_eq!(bytes[2 + 255], tlv_type::PUBLIC_KEY);
    assert_eq!(bytes[2 + 255 + 1], 129);
    assert_eq!(bytes.len(), 2 + 255 + 2 + 129);

    let back = Tlv8::decode(&bytes).unwrap();
    assert_eq!(back.get(tlv_type::PUBLIC_KEY), Some(key.as_slice()));
}

/// A value that is exactly 255 bytes must not be misread as continuing into
/// the next, different-type item.
#[test]
fn exact_255_value_then_other_type() {
    let value: Vec<u8> = vec![7u8; 255];
    let tlv = Tlv8::new().push(tlv_type::PROOF, value.clone()).push_u8(tlv_type::STATE, state::M3);
    let back = Tlv8::decode(&tlv.encode()).unwrap();
    assert_eq!(back.get(tlv_type::PROOF), Some(value.as_slice()));
    assert_eq!(back.get_u8(tlv_type::STATE), Some(state::M3));
}

#[test]
fn empty_value_round_trips() {
    let tlv = Tlv8::new().push(tlv_type::ERROR, Vec::new());
    let bytes = tlv.encode();
    assert_eq!(bytes, vec![tlv_type::ERROR, 0]);
    assert_eq!(Tlv8::decode(&bytes).unwrap().get(tlv_type::ERROR), Some([].as_slice()));
}

#[test]
fn truncated_item_is_an_error() {
    // Length byte says 5 but only 2 value bytes follow.
    assert_eq!(Tlv8::decode(&[0x03, 5, 1, 2]), Err(Tlv8Error::Truncated));
}
