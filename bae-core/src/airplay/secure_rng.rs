//! A `rand_core` 0.6 RNG over the current OS entropy source.
//!
//! The AirPlay crypto crates (`rsa`, `x25519-dalek`, `ed25519-dalek`) are built
//! against `rand_core` 0.6, while bae is on `rand` 0.9. This adapter feeds the
//! older trait from `rand`'s OS RNG so key generation and OAEP padding stay
//! properly random — no fixed seed.

/// A zero-sized CSPRNG that satisfies the `rand_core` 0.6 traits the AirPlay
/// crypto crates require, pulling every byte from the OS RNG.
pub(crate) struct SecureRng;

impl rand_core::RngCore for SecureRng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }

    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        use rand::RngCore as _;
        rand::rng().fill_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl rand_core::CryptoRng for SecureRng {}
