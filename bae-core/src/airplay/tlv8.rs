//! HAP TLV8 — the type-length-value encoding HomeKit pairing messages use.
//!
//! Each item is a one-byte type, a one-byte length, and that many value bytes. A
//! value longer than 255 bytes is split into consecutive items of the *same*
//! type, each ≤255 bytes, which a reader concatenates back together — that's how
//! a 384-byte SRP public key rides in a TLV8 message. This encoding carries the
//! `/pair-setup` and `/pair-verify` bodies (openairplay spec §9; the type
//! registry matches pyatv's `hap_tlv8`, MIT).
//!
//! Only one level of nesting is used (no TLV-in-TLV), matching the pairing flow.

/// The HAP TLV item types the pairing flow uses. Values are the HomeKit standard
/// assignments; only the ones a sender reads or writes are named.
pub mod tlv_type {
    /// The pairing method (`kTLVType_Method`), `0x00` for pair-setup.
    pub const METHOD: u8 = 0x00;
    /// A peer identifier — the controller's pairing id (`kTLVType_Identifier`),
    /// carried inside the encrypted pair-verify sub-messages.
    pub const IDENTIFIER: u8 = 0x01;
    /// The SRP salt `s` sent by the receiver (`kTLVType_Salt`).
    pub const SALT: u8 = 0x02;
    /// An SRP or X25519 public key — the receiver's `B`/ephemeral or the sender's
    /// `A`/ephemeral (`kTLVType_PublicKey`).
    pub const PUBLIC_KEY: u8 = 0x03;
    /// The SRP proof `M1`/`M2` (`kTLVType_Proof`).
    pub const PROOF: u8 = 0x04;
    /// A ChaCha20-Poly1305 ciphertext + tag (`kTLVType_EncryptedData`), carrying
    /// the signed identity in pair-verify M2/M3.
    pub const ENCRYPTED_DATA: u8 = 0x05;
    /// The pairing state machine step, M1–M6 (`kTLVType_State`; pyatv calls it
    /// `SeqNo`).
    pub const STATE: u8 = 0x06;
    /// A receiver-reported error (`kTLVType_Error`).
    pub const ERROR: u8 = 0x07;
    /// An Ed25519 signature over the ephemeral-key exchange (`kTLVType_Signature`).
    pub const SIGNATURE: u8 = 0x0A;
    /// Apple's pairing flags (`kTLVType_Flags`), carrying the transient-pairing
    /// bit.
    pub const FLAGS: u8 = 0x13;
}

/// The transient-pairing flag bit set in [`tlv_type::FLAGS`] (`kPairingFlag_Transient`).
pub const FLAG_TRANSIENT: u32 = 0x10;

/// The HAP pairing state-machine steps carried in [`tlv_type::STATE`].
pub mod state {
    pub const M1: u8 = 0x01;
    pub const M2: u8 = 0x02;
    pub const M3: u8 = 0x03;
    pub const M4: u8 = 0x04;
}

/// A decoded TLV8 message: its items in order, each type paired with its
/// reassembled value (multi-chunk values already concatenated).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tlv8 {
    items: Vec<(u8, Vec<u8>)>,
}

impl Tlv8 {
    pub fn new() -> Self {
        Tlv8 { items: Vec::new() }
    }

    /// Append an item. A caller adds each logical value once; [`Tlv8::encode`]
    /// fragments it on the wire.
    pub fn push(mut self, ty: u8, value: impl Into<Vec<u8>>) -> Self {
        self.items.push((ty, value.into()));
        self
    }

    /// Append a single-byte item (state, method, a small flag).
    pub fn push_u8(self, ty: u8, value: u8) -> Self {
        self.push(ty, vec![value])
    }

    /// The value for `ty`, or `None` if absent. The first item of that type.
    pub fn get(&self, ty: u8) -> Option<&[u8]> {
        self.items
            .iter()
            .find(|(t, _)| *t == ty)
            .map(|(_, v)| v.as_slice())
    }

    /// The single-byte value for `ty` (state/method/error), or `None`.
    pub fn get_u8(&self, ty: u8) -> Option<u8> {
        self.get(ty).and_then(|v| v.first().copied())
    }

    /// Serialize to TLV8 bytes, fragmenting any value longer than 255 bytes into
    /// consecutive items of the same type.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (ty, value) in &self.items {
            if value.is_empty() {
                out.push(*ty);
                out.push(0);
                continue;
            }
            for chunk in value.chunks(255) {
                out.push(*ty);
                out.push(chunk.len() as u8);
                out.extend_from_slice(chunk);
            }
        }
        out
    }

    /// Parse TLV8 bytes, concatenating consecutive items of the same type back
    /// into one value. A truncated item (length byte promising more than remains)
    /// is an error.
    pub fn decode(bytes: &[u8]) -> Result<Tlv8, Tlv8Error> {
        let mut items: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let ty = bytes[i];
            let len = *bytes.get(i + 1).ok_or(Tlv8Error::Truncated)? as usize;
            let start = i + 2;
            let end = start + len;
            let value = bytes.get(start..end).ok_or(Tlv8Error::Truncated)?;

            // A value split across chunks continues the previous item iff that
            // item was the same type and ended on a full 255-byte chunk.
            match items.last_mut() {
                Some((last_ty, last_val))
                    if *last_ty == ty && last_val.len() % 255 == 0 && !last_val.is_empty() =>
                {
                    last_val.extend_from_slice(value);
                }
                _ => items.push((ty, value.to_vec())),
            }
            i = end;
        }
        Ok(Tlv8 { items })
    }
}

/// A malformed TLV8 message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tlv8Error {
    /// An item's length byte promised more bytes than the message held.
    Truncated,
}

impl std::fmt::Display for Tlv8Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tlv8Error::Truncated => write!(f, "truncated TLV8 item"),
        }
    }
}

impl std::error::Error for Tlv8Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_small_items() {
        let tlv = Tlv8::new()
            .push_u8(tlv_type::STATE, state::M1)
            .push_u8(tlv_type::METHOD, 0)
            .push(tlv_type::SALT, vec![1, 2, 3, 4]);
        let bytes = tlv.encode();
        // type,len,value triples.
        assert_eq!(
            bytes,
            vec![0x06, 1, 0x01, 0x00, 1, 0x00, 0x02, 4, 1, 2, 3, 4]
        );

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
        let tlv = Tlv8::new()
            .push(tlv_type::PROOF, value.clone())
            .push_u8(tlv_type::STATE, state::M3);
        let back = Tlv8::decode(&tlv.encode()).unwrap();
        assert_eq!(back.get(tlv_type::PROOF), Some(value.as_slice()));
        assert_eq!(back.get_u8(tlv_type::STATE), Some(state::M3));
    }

    #[test]
    fn empty_value_round_trips() {
        let tlv = Tlv8::new().push(tlv_type::ERROR, Vec::new());
        let bytes = tlv.encode();
        assert_eq!(bytes, vec![tlv_type::ERROR, 0]);
        assert_eq!(
            Tlv8::decode(&bytes).unwrap().get(tlv_type::ERROR),
            Some([].as_slice())
        );
    }

    #[test]
    fn truncated_item_is_an_error() {
        // Length byte says 5 but only 2 value bytes follow.
        assert_eq!(Tlv8::decode(&[0x03, 5, 1, 2]), Err(Tlv8Error::Truncated));
    }
}
