use super::*;
use sha1::Sha1;

fn sha1(data: &[u8]) -> Vec<u8> {
    Sha1::digest(data).to_vec()
}

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.split_whitespace().collect();
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn big(s: &str) -> BigUint {
    BigUint::from_bytes_be(&hex(s))
}

/// RFC 5054 Appendix B test vector (SHA-1, 1024-bit group, I="alice",
/// P="password123"). The published `S` (premaster secret) validates the
/// primitives k/x/u and the modexp end to end, independent of any fake this
/// crate also authors. `A` matches because the vector's `a` is fed in.
#[test]
fn matches_rfc5054_appendix_b() {
    let n = big("EEAF0AB9 ADB38DD6 9C33F80A FA8FC5E8 60726187 75FF3C0B 9EA2314C \
         9C256576 D674DF74 96EA81D3 383B4813 D692C6E0 E0D5D8E2 50B98BE4 \
         8E495C1D 6089DAD1 5DC7D7B4 6154D6B6 CE8EF4AD 69B15D49 82559B29 \
         7BCF1885 C529F566 660E57EC 68EDBC3C 05726CC0 2FD4CBF4 976EAA9A \
         FD5138FE 8376435B 9FC61D2F C0EB06E3");
    let group = SrpGroup { n, g: BigUint::from(2u8) };
    let a = big("60975527 035CF2AD 1989806F 0407210B C81EDC04 E2762A56 AFD529DD DA2D4393");
    let salt = hex("BEB25379 D1A8581E B5A72767 3A2441EE");
    let b_pub = hex("BD0C6151 2C692C0C B6D041FA 01BB152D 4916A1E7 7AF46AE1 05393011 \
         BAF38964 DC46A067 0DD125B9 5A981652 236F99D9 B681CBF8 7837EC99 \
         6C6DA044 53728610 D0C6DDB5 8B318885 D7D82C7F 8DEB75CE 7BD4FBAA \
         37089E6F 9C6059F3 88838E7A 00030B33 1EB76840 910440B1 B27AAEAE \
         EB4012B7 D7665238 A8E3FB00 4B117B58");

    let mut client = SrpClient::new(group, sha1, b"alice", b"password123", a);

    // A matches the published value.
    assert_eq!(
        client.public_a(),
        hex("61D5E490 F6F1B795 47B0704C 436F523D D0E560F0 C64115BB 72557EC4 \
             4352E890 3211C046 92272D8B 2D1A5358 A2CF1B6E 0BFCF99F 921530EC \
             8E393561 79EAE45E 42BA92AE ACED8251 71E1E8B9 AF6D9C03 E1327F44 \
             BE087EF0 6530E69F 66615261 EEF54073 CA11CF58 58F0EDFD FE15EFEA \
             B349EF5D 76988A36 72FAC47B 0769447B")
    );

    client.process(&salt, &b_pub).unwrap();

    // The shared key is H(S); check against H(published S) computed here with
    // an independent one-liner, so the derivation isn't taken on faith.
    let published_s = hex("B0DC82BA BCF30674 AE450C02 87745E79 90A3381F 63B387AA F271A10D \
         233861E3 59B48220 F7C4693C 9AE12B0A 6F67809F 0876E2D0 13800D6C \
         41BB59B6 D5979B5C 00A172B4 A2A5903A 0BDCAF8A 709585EB 2AFAFA8F \
         3499B200 210DCC1F 10EB3394 3CD67FC8 8A2F39A4 BE5BEC4E C0A3212D \
         C346D7E4 74B29EDE 8A469FFE CA686E5A");
    assert_eq!(
        client.shared_key().unwrap(),
        sha1(&published_s).as_slice(),
        "K = H(S) must derive from the RFC 5054 premaster secret"
    );
}

/// The 3072-bit HomeKit group parses and A is in range.
#[test]
fn homekit_group_produces_a_public_value() {
    let a = BigUint::from_bytes_be(&[0x42u8; 32]);
    let client = SrpClient::new(SrpGroup::rfc5054_3072(), sha512, b"Pair-Setup", b"3939", a);
    let a_pub = client.public_a();
    // A is a 3072-bit value: at most 384 bytes, and not zero.
    assert!(!a_pub.is_empty() && a_pub.len() <= 384);
}

#[test]
fn pad_left_fills_to_width() {
    assert_eq!(pad(4, &BigUint::from(0x01u8)), vec![0, 0, 0, 1]);
    assert_eq!(pad(2, &BigUint::from(0x0102u16)), vec![1, 2]);
    // Already at or over width: unchanged.
    assert_eq!(pad(1, &BigUint::from(0x0102u16)), vec![1, 2]);
}
