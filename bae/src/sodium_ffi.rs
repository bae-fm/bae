//! Minimal FFI bindings to libsodium for XChaCha20-Poly1305 AEAD
//!
//! Requires libsodium system library:
//! - macOS: `brew install libsodium`
//! - Linux: `apt install libsodium-dev`

use libc::{c_int, c_uchar, c_ulonglong};

pub const NPUBBYTES: usize = 24; // nonce size
pub const ABYTES: usize = 16; // auth tag size

extern "C" {
    pub fn sodium_init() -> c_int;

    pub fn crypto_aead_xchacha20poly1305_ietf_encrypt(
        c: *mut c_uchar,
        clen_p: *mut c_ulonglong,
        m: *const c_uchar,
        mlen: c_ulonglong,
        ad: *const c_uchar,
        adlen: c_ulonglong,
        nsec: *const c_uchar,
        npub: *const c_uchar,
        k: *const c_uchar,
    ) -> c_int;

    pub fn crypto_aead_xchacha20poly1305_ietf_decrypt(
        m: *mut c_uchar,
        mlen_p: *mut c_ulonglong,
        nsec: *mut c_uchar,
        c: *const c_uchar,
        clen: c_ulonglong,
        ad: *const c_uchar,
        adlen: c_ulonglong,
        npub: *const c_uchar,
        k: *const c_uchar,
    ) -> c_int;

    pub fn randombytes_buf(buf: *mut c_uchar, size: usize);
}
