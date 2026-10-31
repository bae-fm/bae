//! Giving back the memory the open library's store keeps cached.

use super::*;

forward! { async this => {
    /// Give back the memory the library's store keeps cached. Phones call it
    /// when the OS warns of low memory and when the app leaves the screen:
    /// Android from `onTrimMemory`. Safe at any time; it never waits for a
    /// write. Reads after it load what they need again.
    fn release_memory() -> () {
        Ok(this.services.release_memory().await?)
    }
} }
