//! bae's infrastructure: the pieces with no library or music vocabulary of
//! their own, which every other bae crate builds on and which depend on
//! nothing else in bae.

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
