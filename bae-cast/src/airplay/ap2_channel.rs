//! The encrypted AirPlay 2 control channel and the keys derived from the
//! transient pair-setup secret.
//!
//! Once transient pair-setup establishes the SRP session key, the RTSP control
//! connection is wrapped in the HomeKit secure transport: each message is split
//! into blocks, and every block is `2-byte little-endian length ‖ ChaCha20-Poly1305(counter
//! nonce, block, aad = the length bytes) ‖ 16-byte tag`. The counter increments
//! per block, separately per direction. The write/read keys and the audio
//! session key (`shk`, sent to the receiver in the SETUP streams plist) are all
//! HKDF-SHA512 expansions of the session secret under fixed salt/info labels
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
    hk.expand(info, &mut okm).expect("32 is a valid HKDF length");
    okm
}

/// The control-channel keys derived from the pair-setup session secret: the
/// sender's write key (controller → accessory) and read key (accessory →
/// controller).
pub(crate) fn control_keys(shared: &[u8]) -> ([u8; 32], [u8; 32]) {
    let write = hkdf32(b"Control-Salt", shared, b"Control-Write-Encryption-Key");
    let read = hkdf32(b"Control-Salt", shared, b"Control-Read-Encryption-Key");
    (write, read)
}

/// The audio session key `shk` the SETUP streams plist carries to the receiver.
/// The 12-byte ChaCha20-Poly1305 nonce AirPlay 2 uses everywhere: four zero
/// bytes, then 8 bytes of `tail` (a little-endian counter or a message label).
pub(crate) fn nonce_with_tail(tail: &[u8; 8]) -> Nonce {
    let mut bytes = [0u8; 12];
    bytes[4..].copy_from_slice(tail);
    Nonce::from(bytes)
}

pub(crate) fn audio_key(shared: &[u8]) -> [u8; 32] {
    hkdf32(b"Events-Salt", shared, b"Events-Write-Encryption-Key")
}

/// A failure on the encrypted channel — a block whose tag didn't verify.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ChannelError {
    #[error("encrypted control channel: block failed to decrypt")]
    Decrypt,
}

/// The sender's half of the HomeKit secure transport over one control
/// connection: it seals outgoing bytes and reassembles/decrypts incoming ones,
/// tracking a counter per direction.
pub(crate) struct HapChannel {
    write_cipher: ChaCha20Poly1305,
    read_cipher: ChaCha20Poly1305,
    write_counter: u64,
    read_counter: u64,
    /// Bytes read from the socket, awaiting complete blocks.
    in_buf: Vec<u8>,
}

impl HapChannel {
    /// Build the channel from the pair-setup session secret.
    pub(crate) fn from_shared_secret(shared: &[u8]) -> Self {
        let (write_key, read_key) = control_keys(shared);
        Self::from_keys(&write_key, &read_key)
    }

    /// Build the channel from explicit write/read keys (the receiver side of a
    /// test swaps them).
    pub(crate) fn from_keys(write_key: &[u8; 32], read_key: &[u8; 32]) -> Self {
        HapChannel {
            write_cipher: ChaCha20Poly1305::new(Key::from_slice(write_key)),
            read_cipher: ChaCha20Poly1305::new(Key::from_slice(read_key)),
            write_counter: 0,
            read_counter: 0,
            in_buf: Vec::new(),
        }
    }

    /// Seal `plaintext` into one or more length-prefixed encrypted blocks.
    pub(crate) fn seal(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(plaintext.len() + 18);
        for block in plaintext.chunks(MAX_BLOCK) {
            let aad = (block.len() as u16).to_le_bytes();
            let sealed = self
                .write_cipher
                .encrypt(
                    &nonce_with_tail(&self.write_counter.to_le_bytes()),
                    Payload { msg: block, aad: &aad },
                )
                .expect("ChaCha20-Poly1305 seal cannot fail for a valid key");
            self.write_counter = self.write_counter.wrapping_add(1);
            out.extend_from_slice(&aad);
            out.extend_from_slice(&sealed);
        }
        out
    }

    /// Add bytes read from the socket to the reassembly buffer.
    pub(crate) fn feed(&mut self, data: &[u8]) {
        self.in_buf.extend_from_slice(data);
    }

    /// Decrypt the next complete block, or `None` if the buffer holds only a
    /// partial one. Call repeatedly to drain everything fed so far.
    pub(crate) fn next_block(&mut self) -> Result<Option<Vec<u8>>, ChannelError> {
        if self.in_buf.len() < 2 {
            return Ok(None);
        }
        let len = u16::from_le_bytes([self.in_buf[0], self.in_buf[1]]) as usize;
        let frame_len = 2 + len + 16;
        if self.in_buf.len() < frame_len {
            return Ok(None);
        }
        let aad = [self.in_buf[0], self.in_buf[1]];
        #[expect(
            clippy::map_err_ignore,
            reason = "an AEAD failure is opaque by design: it says only that the tag did not verify"
        )]
        let plaintext = self
            .read_cipher
            .decrypt(
                &nonce_with_tail(&self.read_counter.to_le_bytes()),
                Payload { msg: &self.in_buf[2..frame_len], aad: &aad },
            )
            .map_err(|_| ChannelError::Decrypt)?;
        self.read_counter = self.read_counter.wrapping_add(1);
        self.in_buf.drain(..frame_len);
        Ok(Some(plaintext))
    }
}

#[cfg(test)]
#[path = "ap2_channel_tests.rs"]
mod tests;
