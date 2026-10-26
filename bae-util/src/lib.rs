//! bae's infrastructure: the pieces with no library or music vocabulary of
//! their own, which every other bae crate builds on and which depend on
//! nothing else in bae.

/// Gates every item in the block to the desktop targets, so the predicate is
/// written once for a run of modules or re-exports instead of above each one.
/// Mobile (iOS, Android) is a sync and playback client: no import pipeline, no
/// signal extraction, no metadata providers.
#[macro_export]
macro_rules! desktop_only {
    ($($item:item)*) => {
        $(
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            $item
        )*
    };
}

pub mod content_type;
pub mod content_type_hint;
pub mod duration;
pub mod error_text;
pub mod fs;
pub mod http;
#[cfg(unix)]
pub mod open_file_limit;
pub mod retry;
pub mod server;
pub mod session_cache;
pub mod stored_image;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod text_encoding;
pub mod worker_thread;
