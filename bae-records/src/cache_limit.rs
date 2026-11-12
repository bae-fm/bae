//! The choices for how much audio a device keeps cached before the least
//! recently played goes: Settings → Storage's Keep up to.

/// How much of a device's cache the audio and a release's other files may
/// take, in decimal gigabytes, as both platforms' byte formatters print them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLimit {
    Gb1,
    Gb5,
    Gb10,
    Gb20,
    Gb50,
}

impl CacheLimit {
    /// Every choice, smallest first.
    pub const ALL: [CacheLimit; 5] =
        [CacheLimit::Gb1, CacheLimit::Gb5, CacheLimit::Gb10, CacheLimit::Gb20, CacheLimit::Gb50];

    /// What a library starts at on a device, and after a reset or a restore.
    pub const DEFAULT: CacheLimit = CacheLimit::Gb20;

    /// The limit in bytes.
    pub const fn bytes(self) -> u64 {
        let gigabytes: u64 = match self {
            CacheLimit::Gb1 => 1,
            CacheLimit::Gb5 => 5,
            CacheLimit::Gb10 => 10,
            CacheLimit::Gb20 => 20,
            CacheLimit::Gb50 => 50,
        };
        gigabytes * 1_000_000_000
    }

    /// The choice whose limit is `bytes`; none when no choice names it.
    pub fn from_bytes(bytes: u64) -> Option<Self> {
        Self::ALL.into_iter().find(|limit| limit.bytes() == bytes)
    }
}

#[cfg(test)]
#[path = "cache_limit_tests.rs"]
mod tests;
