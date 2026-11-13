//! HAP TLV8 — the type-length-value encoding HomeKit pairing messages use.
//!
//! Each item is a one-byte type, a one-byte length, and that many value bytes. A
//! value longer than 255 bytes is split into consecutive items of the *same*
//! type, each ≤255 bytes, which a reader concatenates back together — that's how
//! a 384-byte SRP public key rides in a TLV8 message. This encoding carries the
//! `/pair-setup` bodies (openairplay spec §9; the type
//! registry matches pyatv's `hap_tlv8`, MIT).
//!
//! Only one level of nesting is used (no TLV-in-TLV), matching the pairing flow.

/// The HAP TLV item types the pairing flow uses. Values are the HomeKit standard
/// assignments; only the ones a sender reads or writes are named.
pub(crate) mod tlv_type {
    /// The pairing method (`kTLVType_Method`), `0x00` for pair-setup.
    pub(crate) const METHOD: u8 = 0x00;
    /// The SRP salt `s` sent by the receiver (`kTLVType_Salt`).
    pub(crate) const SALT: u8 = 0x02;
    /// An SRP or X25519 public key — the receiver's `B`/ephemeral or the sender's
    /// `A`/ephemeral (`kTLVType_PublicKey`).
    pub(crate) const PUBLIC_KEY: u8 = 0x03;
    /// The SRP proof `M1`/`M2` (`kTLVType_Proof`).
    pub(crate) const PROOF: u8 = 0x04;
    /// The pairing state machine step, M1–M6 (`kTLVType_State`; pyatv calls it
    /// `SeqNo`).
    pub(crate) const STATE: u8 = 0x06;
    /// A receiver-reported error (`kTLVType_Error`).
    pub(crate) const ERROR: u8 = 0x07;
    /// Apple's pairing flags (`kTLVType_Flags`), carrying the transient-pairing
    /// bit.
    pub(crate) const FLAGS: u8 = 0x13;
}

/// The transient-pairing flag bit set in [`tlv_type::FLAGS`] (`kPairingFlag_Transient`).
pub(crate) const FLAG_TRANSIENT: u32 = 0x10;

/// The HAP pairing state-machine steps carried in [`tlv_type::STATE`].
pub(crate) mod state {
    pub(crate) const M1: u8 = 0x01;
    pub(crate) const M2: u8 = 0x02;
    pub(crate) const M3: u8 = 0x03;
    pub(crate) const M4: u8 = 0x04;
}

/// A decoded TLV8 message: its items in order, each type paired with its
/// reassembled value (multi-chunk values already concatenated).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Tlv8 {
    items: Vec<(u8, Vec<u8>)>,
}

impl Tlv8 {
    pub(crate) fn new() -> Self {
        Tlv8 { items: Vec::new() }
    }

    /// Append an item. A caller adds each logical value once; [`Tlv8::encode`]
    /// fragments it on the wire.
    pub(crate) fn push(mut self, ty: u8, value: impl Into<Vec<u8>>) -> Self {
        self.items.push((ty, value.into()));
        self
    }

    /// Append a single-byte item (state, method, a small flag).
    pub(crate) fn push_u8(self, ty: u8, value: u8) -> Self {
        self.push(ty, vec![value])
    }

    /// The value for `ty`, or `None` if absent. The first item of that type.
    pub(crate) fn get(&self, ty: u8) -> Option<&[u8]> {
        self.items.iter().find(|(t, _)| *t == ty).map(|(_, v)| v.as_slice())
    }

    /// The single-byte value for `ty` (state/method/error), or `None`.
    pub(crate) fn get_u8(&self, ty: u8) -> Option<u8> {
        self.get(ty).and_then(|v| v.first().copied())
    }

    /// Serialize to TLV8 bytes, fragmenting any value longer than 255 bytes into
    /// consecutive items of the same type.
    pub(crate) fn encode(&self) -> Vec<u8> {
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
    pub(crate) fn decode(bytes: &[u8]) -> Result<Tlv8, Tlv8Error> {
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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Tlv8Error {
    /// An item's length byte promised more bytes than the message held.
    #[error("truncated TLV8 item")]
    Truncated,
}

#[cfg(test)]
#[path = "tlv8_tests.rs"]
mod tests;
