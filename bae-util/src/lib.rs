//! bae's infrastructure: the pieces with no library or music vocabulary of
//! their own, which every other bae crate builds on and which depend on
//! nothing else in bae.

pub mod audio_container;
pub mod content_type;
pub mod content_type_hint;
pub mod database_failure;
pub mod duration;
pub mod error_text;
pub mod fs;
pub mod http;
pub mod logging;
pub mod natural_order;
#[cfg(unix)]
pub mod open_file_limit;
#[cfg(any(test, feature = "test-utils"))]
pub mod open_files_peak;
pub mod path_text;
pub mod retry;
pub mod sample_time;
pub mod search_text;
pub mod server;
pub mod session_cache;
pub mod stall;
pub mod stored_image;
pub mod task_failure;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_blocking;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_channels;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_cover_art;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_host;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_ids;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_logs;
#[cfg(desktop)]
pub mod text_encoding;
pub mod time_ordered_ids;
pub mod worker_thread;
pub mod xml;
