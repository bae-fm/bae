//! The live reads the screens pull: each read follows one of the library's
//! views or tables on a task of the library's, resolves what it reads into
//! the shape a screen shows, and moves to another id or window in place. The
//! library closing ends every one.

mod detail;
mod events;
mod queue_upcoming;
#[cfg(desktop)]
mod release_files;
mod storage;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod storage_inspector;

pub use detail::*;
pub use events::*;
pub use queue_upcoming::*;
#[cfg(desktop)]
pub use release_files::*;
pub use storage::*;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use storage_inspector::*;

use crate::library::views::LibraryViews;
use crate::library::LibraryManager;
