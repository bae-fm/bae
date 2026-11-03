use super::*;

fn dict(entries: &[(&str, Plist)]) -> Plist {
    Plist::Dict(entries.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}

/// A small dict encodes to a well-formed bplist and decodes back unchanged.
#[test]
fn round_trips_a_flat_dict() {
    let value = dict(&[("rate", Plist::Integer(1)), ("rtpTime", Plist::Integer(0))]);
    let bytes = encode(&value);
    assert_eq!(&bytes[..8], b"bplist00");
    assert_eq!(decode(&bytes).unwrap(), value);
}

/// Nested arrays, data blobs, big integers, and long strings survive a round
/// trip — the shapes the SETUP/SETPEERS bodies use.
#[test]
fn round_trips_nested_structures() {
    let value = dict(&[
        (
            "streams",
            Plist::Array(vec![dict(&[
                ("type", Plist::Integer(96)),
                ("shk", Plist::Data(vec![0xAB; 32])),
                ("latencyMax", Plist::Integer(88_200)),
                ("timingProtocol", Plist::String("NTP".to_string())),
            ])]),
        ),
        ("sessionUUID", Plist::String("3195C737-1E6E-4487-BECB-4D287B7C7626".to_string())),
        ("networkTimeTimelineID", Plist::Integer(0x1122_3344_5566_7788)),
    ]);
    let bytes = encode(&value);
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded, value);
    // Reach into the decoded tree the way the response parser does.
    let shk = decoded
        .get("streams")
        .and_then(|s| match s {
            Plist::Array(items) => items.first(),
            _ => None,
        })
        .and_then(|s| s.get("shk"))
        .unwrap();
    assert_eq!(shk, &Plist::Data(vec![0xAB; 32]));
}

/// An array of IP strings — the SETPEERS body — round-trips.
#[test]
fn round_trips_string_array() {
    let value = Plist::Array(vec![
        Plist::String("10.0.0.2".to_string()),
        Plist::String("10.0.0.9".to_string()),
    ]);
    assert_eq!(decode(&encode(&value)).unwrap(), value);
}

/// The trailer records the object count and a top index of 0.
#[test]
fn trailer_is_well_formed() {
    let bytes = encode(&dict(&[("rate", Plist::Integer(0))]));
    let trailer = &bytes[bytes.len() - 32..];
    // dict + key string "rate" + value int 0 = 3 objects.
    assert_eq!(u64::from_be_bytes(trailer[8..16].try_into().unwrap()), 3);
    assert_eq!(u64::from_be_bytes(trailer[16..24].try_into().unwrap()), 0);
}

/// A booleans-and-reals value decodes to what was encoded.
#[test]
fn round_trips_bool_and_real() {
    let value =
        dict(&[("on", Plist::Bool(true)), ("off", Plist::Bool(false)), ("gain", Plist::Real(0.5))]);
    assert_eq!(decode(&encode(&value)).unwrap(), value);
}

#[test]
fn rejects_non_bplist() {
    assert_eq!(decode(b"not a plist").unwrap_err(), BplistError::BadHeader);
}

/// A payload laid out by hand: `objects` packed after the header, a
/// one-byte offset table, and a trailer with one-byte refs. `num_objects`
/// is what the trailer claims, which a malformed payload need not match.
fn assemble(objects: &[&[u8]], num_objects: u64) -> Vec<u8> {
    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::new();
    for object in objects {
        offsets.push(u8::try_from(out.len()).expect("test payload fits one-byte offsets"));
        out.extend_from_slice(object);
    }
    let table_offset = out.len() as u64;
    out.extend_from_slice(&offsets);
    out.extend_from_slice(&[0; 6]);
    out.push(1);
    out.push(1);
    out.extend_from_slice(&num_objects.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&table_offset.to_be_bytes());
    out
}

#[test]
fn rejects_an_object_count_the_payload_cannot_hold() {
    let bytes = assemble(&[&[0x09]], u64::MAX);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::BadTrailer);
}

#[test]
fn rejects_a_real_of_a_width_that_is_not_four_or_eight_bytes() {
    let bytes = assemble(&[&[0x21, 0x00, 0x00]], 1);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::BadWidth(0x21));
}

#[test]
fn rejects_an_integer_wider_than_eight_bytes() {
    let bytes = assemble(&[&[0x1F]], 1);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::BadWidth(0x1F));
}

#[test]
fn rejects_a_collection_count_the_payload_cannot_hold() {
    // An array whose inline count is 2^64 - 1 refs.
    let array = [0xAF, 0x13, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let bytes = assemble(&[&array], 1);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::Truncated);
}

#[test]
fn rejects_a_utf16_string_longer_than_the_payload() {
    let string = [0x6F, 0x13, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let bytes = assemble(&[&string], 1);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::Truncated);
}

#[test]
fn rejects_an_array_that_contains_itself() {
    let bytes = assemble(&[&[0xA1, 0x00]], 1);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::TooDeep);
}

/// Twenty arrays, each holding the next one twice, name 2^20 objects
/// through shared references while the payload holds twenty-one.
#[test]
fn rejects_shared_references_that_multiply_into_too_many_objects() {
    let arrays: Vec<[u8; 3]> = (1..=20u8).map(|next| [0xA2, next, next]).collect();
    let mut objects: Vec<&[u8]> = arrays.iter().map(|array| array.as_slice()).collect();
    objects.push(&[0x09]);
    let bytes = assemble(&objects, objects.len() as u64);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::TooManyObjects);
}

#[test]
fn rejects_a_dict_key_that_is_not_a_string() {
    // {1: true}
    let bytes = assemble(&[&[0xD1, 0x01, 0x02], &[0x10, 0x01], &[0x09]], 3);
    assert_eq!(decode(&bytes).unwrap_err(), BplistError::NonStringKey);
}
