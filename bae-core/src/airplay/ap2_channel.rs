//! The encrypted AirPlay 2 control channel and the keys derived from pair-verify.
//!
//! Once pair-verify establishes a shared secret, the RTSP control connection is
//! wrapped in the HomeKit secure transport: each message is split into blocks,
//! and every block is `2-byte little-endian length ‖ ChaCha20-Poly1305(counter
//! nonce, block, aad = the length bytes) ‖ 16-byte tag`. The counter increments
//! per block, separately per direction. The write/read keys and the audio
//! session key (`shk`, sent to the receiver in the SETUP streams plist) are all
//! HKDF-SHA512 expansions of the pair-verify secret under fixed salt/info labels
//! (semantics from the MIT `airplay2-rs` and pyatv).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha512;

/// A HAP block carries at most a `u16` length of plaintext.
const MAX_BLOCK: usize = u16::MAX as usize;

/// HKDF-SHA512 expand of `ikm` under `salt`/`info` to a 32-byte key.
pub(crate) fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha512>::new(Some(salt), ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("32 is a valid HKDF length");
    okm
}

/// The control-channel keys derived from the pair-verify shared secret: the
/// sender's write key (controller → accessory) and read key (accessory →
/// controller).
pub fn control_keys(shared: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let write = hkdf32(b"Control-Salt", shared, b"Control-Write-Encryption-Key");
    let read = hkdf32(b"Control-Salt", shared, b"Control-Read-Encryption-Key");
    (write, read)
}

/// The audio session key `shk` the SETUP streams plist carries to the receiver.
pub fn audio_key(shared: &[u8; 32]) -> [u8; 32] {
    hkdf32(b"Events-Salt", shared, b"Events-Write-Encryption-Key")
}

/// A failure on the encrypted channel — a block whose tag didn't verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelError {
    Decrypt,
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "encrypted control channel: block failed to decrypt")
    }
}

impl std::error::Error for ChannelError {}

/// The sender's half of the HomeKit secure transport over one control
/// connection: it seals outgoing bytes and reassembles/decrypts incoming ones,
/// tracking a counter per direction.
pub struct HapChannel {
    write_cipher: ChaCha20Poly1305,
    read_cipher: ChaCha20Poly1305,
    write_counter: u64,
    read_counter: u64,
    /// Bytes read from the socket, awaiting complete blocks.
    in_buf: Vec<u8>,
}

impl HapChannel {
    /// Build the channel from the pair-verify shared secret.
    pub fn from_shared_secret(shared: &[u8; 32]) -> Self {
        let (write_key, read_key) = control_keys(shared);
        Self::from_keys(&write_key, &read_key)
    }

    /// Build the channel from explicit write/read keys (the receiver side of a
    /// test swaps them).
    pub fn from_keys(write_key: &[u8; 32], read_key: &[u8; 32]) -> Self {
        HapChannel {
            write_cipher: ChaCha20Poly1305::new(Key::from_slice(write_key)),
            read_cipher: ChaCha20Poly1305::new(Key::from_slice(read_key)),
            write_counter: 0,
            read_counter: 0,
            in_buf: Vec::new(),
        }
    }

    fn nonce(counter: u64) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&counter.to_le_bytes());
        nonce
    }

    /// Seal `plaintext` into one or more length-prefixed encrypted blocks.
    pub fn seal(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(plaintext.len() + 18);
        for block in plaintext.chunks(MAX_BLOCK) {
            let aad = (block.len() as u16).to_le_bytes();
            let nonce = Self::nonce(self.write_counter);
            let sealed = self
                .write_cipher
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: block,
                        aad: &aad,
                    },
                )
                .expect("ChaCha20-Poly1305 seal cannot fail for a valid key");
            self.write_counter = self.write_counter.wrapping_add(1);
            out.extend_from_slice(&aad);
            out.extend_from_slice(&sealed);
        }
        out
    }

    /// Add bytes read from the socket to the reassembly buffer.
    pub fn feed(&mut self, data: &[u8]) {
        self.in_buf.extend_from_slice(data);
    }

    /// Decrypt the next complete block, or `None` if the buffer holds only a
    /// partial one. Call repeatedly to drain everything fed so far.
    pub fn next_block(&mut self) -> Result<Option<Vec<u8>>, ChannelError> {
        if self.in_buf.len() < 2 {
            return Ok(None);
        }
        let len = u16::from_le_bytes([self.in_buf[0], self.in_buf[1]]) as usize;
        let frame_len = 2 + len + 16;
        if self.in_buf.len() < frame_len {
            return Ok(None);
        }
        let aad = [self.in_buf[0], self.in_buf[1]];
        let nonce = Self::nonce(self.read_counter);
        let plaintext = self
            .read_cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &self.in_buf[2..frame_len],
                    aad: &aad,
                },
            )
            .map_err(|_| ChannelError::Decrypt)?;
        self.read_counter = self.read_counter.wrapping_add(1);
        self.in_buf.drain(..frame_len);
        Ok(Some(plaintext))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A message sealed by the sender is read back by a peer that swaps write/read
    /// keys — the HomeKit-transport role reversal — proving the framing, nonce
    /// counters, and length-as-AAD all agree.
    #[test]
    fn seal_reassembles_across_the_role_reversal() {
        let shared = [0x42u8; 32];
        let (write, read) = control_keys(&shared);
        let mut sender = HapChannel::from_keys(&write, &read);
        // The receiver's write key is the sender's read key, and vice versa.
        let mut receiver = HapChannel::from_keys(&read, &write);

        let msg = b"SETUP rtsp://x/1 RTSP/1.0\r\nCSeq: 1\r\n\r\n";
        let sealed = sender.seal(msg);
        // The length prefix is the plaintext length, and there's a 16-byte tag.
        assert_eq!(
            u16::from_le_bytes([sealed[0], sealed[1]]) as usize,
            msg.len()
        );
        assert_eq!(sealed.len(), 2 + msg.len() + 16);

        receiver.feed(&sealed);
        let recovered = receiver.next_block().unwrap().unwrap();
        assert_eq!(recovered, msg);
        assert!(receiver.next_block().unwrap().is_none());
    }

    /// Counters advance per block, so successive messages use distinct nonces and
    /// still decrypt in order.
    #[test]
    fn counters_advance_per_block() {
        let (write, read) = control_keys(&[1u8; 32]);
        let mut sender = HapChannel::from_keys(&write, &read);
        let mut receiver = HapChannel::from_keys(&read, &write);

        let a = sender.seal(b"first");
        let b = sender.seal(b"second");
        // Deliver out of one buffer to prove reassembly, in order.
        receiver.feed(&a);
        receiver.feed(&b);
        assert_eq!(receiver.next_block().unwrap().unwrap(), b"first");
        assert_eq!(receiver.next_block().unwrap().unwrap(), b"second");
    }

    /// A partial frame yields nothing until the rest arrives.
    #[test]
    fn partial_frame_waits_for_more() {
        let (write, read) = control_keys(&[7u8; 32]);
        let mut sender = HapChannel::from_keys(&write, &read);
        let mut receiver = HapChannel::from_keys(&read, &write);
        let sealed = sender.seal(b"hello world");

        receiver.feed(&sealed[..5]);
        assert!(receiver.next_block().unwrap().is_none());
        receiver.feed(&sealed[5..]);
        assert_eq!(receiver.next_block().unwrap().unwrap(), b"hello world");
    }

    /// The HKDF primitive matches RFC 5869's published Test Case 1 (SHA-256), so
    /// the derivation the control/audio keys ride on is validated against the
    /// standard, not only against itself.
    #[test]
    fn hkdf_matches_rfc5869_test_case_1() {
        use hkdf::Hkdf;
        use sha2::Sha256;

        let ikm = [0x0bu8; 22];
        let salt: Vec<u8> = (0x00u8..=0x0c).collect();
        let info: Vec<u8> = (0xf0u8..=0xf9).collect();

        let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
        let mut okm = [0u8; 42];
        hk.expand(&info, &mut okm).unwrap();

        let expected = hex::decode(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
             34007208d5b887185865",
        )
        .unwrap();
        assert_eq!(okm.as_slice(), expected.as_slice());
    }

    /// The audio key and control keys are distinct expansions of one secret.
    #[test]
    fn derived_keys_are_distinct() {
        let shared = [0x9Au8; 32];
        let (write, read) = control_keys(&shared);
        let shk = audio_key(&shared);
        assert_ne!(write, read);
        assert_ne!(write, shk);
        assert_ne!(read, shk);
    }
}
