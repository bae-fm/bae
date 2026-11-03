//! Ids for test fixtures.

/// A stable v4 UUID for a fixture moniker.
///
/// coven validates every synced row's primary key as a canonical RFC 4122 v4
/// UUID (`RowIdentity::IndependentUuid`) — which is what bae's real ids are, so
/// fixtures must carry UUIDs too. Tests that name a row by a readable moniker
/// (`"test-artist-id"`) or mint one per index (`format!("track-{i}")`) get their
/// id through this: the moniker stays visible at the call site, and the same
/// moniker always maps to the same id within and across runs.
pub fn test_uuid(moniker: &str) -> String {
    // FNV-1a over the moniker, run under four seeds for the 128 bits a UUID
    // needs. Any stable spread works — the value only has to be a well-formed
    // v4 UUID and collision-free across a test's handful of monikers.
    let word = |seed: u64| -> u64 {
        let mut hash = seed;
        for byte in moniker.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    };
    let (high, low) = (word(0xcbf2_9ce4_8422_2325), word(0x9e37_79b9_7f4a_7c15));
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (high >> 32) as u32,
        (high >> 16) as u16,
        high & 0x0fff,
        (low >> 48) & 0x0fff,
        low & 0xffff_ffff_ffff_u64,
    )
}
