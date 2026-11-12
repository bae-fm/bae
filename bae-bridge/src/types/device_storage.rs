//! What this device keeps for the open library, as Settings → Storage shows
//! it. Mirrors `DeviceStorage`.

use bae_library::{DeviceStorage, StorageKind, StoragePart};
use bae_records::CacheLimit;

/// What the device keeps, with each part's bytes: the page draws the bar's
/// segments in proportion to each part's bytes over `total`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDeviceStorage {
    pub total: u64,
    /// The kinds that show, in the fixed order Pinned, Cache, Library, From
    /// before the reset.
    pub parts: Vec<BridgeStoragePart>,
    /// The cache's bytes, for Clear Cache's row.
    pub cache: u64,
    pub can_clear_cache: bool,
    /// How much audio the cache keeps.
    pub cache_limit: BridgeCacheLimit,
    /// Every choice for it, smallest first.
    pub cache_limits: Vec<BridgeCacheLimit>,
    /// The bytes of the data from before a reset, while there is some.
    pub before_reset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeStoragePart {
    pub kind: BridgeStorageKind,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStorageKind {
    Pinned,
    Cache,
    Library,
    BeforeReset,
}

/// One choice of how much audio the cache keeps, with its limit in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCacheLimit {
    pub choice: BridgeCacheLimitChoice,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCacheLimitChoice {
    Gb1,
    Gb5,
    Gb10,
    Gb20,
    Gb50,
}

impl BridgeDeviceStorage {
    pub(crate) fn from_core(storage: DeviceStorage) -> Self {
        Self {
            total: storage.total,
            parts: storage.parts.into_iter().map(BridgeStoragePart::from_core).collect(),
            cache: storage.cache,
            can_clear_cache: storage.can_clear_cache,
            cache_limit: BridgeCacheLimit::from_core(storage.cache_limit),
            cache_limits: CacheLimit::ALL.into_iter().map(BridgeCacheLimit::from_core).collect(),
            before_reset: storage.before_reset,
        }
    }
}

impl BridgeStoragePart {
    fn from_core(part: StoragePart) -> Self {
        Self {
            kind: match part.kind {
                StorageKind::Pinned => BridgeStorageKind::Pinned,
                StorageKind::Cache => BridgeStorageKind::Cache,
                StorageKind::Library => BridgeStorageKind::Library,
                StorageKind::BeforeReset => BridgeStorageKind::BeforeReset,
            },
            bytes: part.bytes,
        }
    }
}

impl BridgeCacheLimit {
    fn from_core(limit: CacheLimit) -> Self {
        Self { choice: BridgeCacheLimitChoice::from_core(limit), bytes: limit.bytes() }
    }
}

impl BridgeCacheLimitChoice {
    fn from_core(limit: CacheLimit) -> Self {
        match limit {
            CacheLimit::Gb1 => Self::Gb1,
            CacheLimit::Gb5 => Self::Gb5,
            CacheLimit::Gb10 => Self::Gb10,
            CacheLimit::Gb20 => Self::Gb20,
            CacheLimit::Gb50 => Self::Gb50,
        }
    }

    pub(crate) fn into_core(self) -> CacheLimit {
        match self {
            Self::Gb1 => CacheLimit::Gb1,
            Self::Gb5 => CacheLimit::Gb5,
            Self::Gb10 => CacheLimit::Gb10,
            Self::Gb20 => CacheLimit::Gb20,
            Self::Gb50 => CacheLimit::Gb50,
        }
    }
}
