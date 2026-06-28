//! bae's storage helpers around coven's cloud backends + encrypted layout. The
//! locality-aware blob read/write, the pin/unpin queue, and the offline-home
//! stub all live behind [`coven::CovenHandle`] now; what remains here is the
//! cloud-provider setup re-export and the deferred local-cleanup manifest.
pub mod cloud {
    //! bae's view of coven's cloud backend surface, re-exported flat. coven owns
    //! the providers and the `CloudHome` contract; bae implements `CloudHome` /
    //! `CloudKitOps` against these and constructs `S3CloudHome` directly.
    // The host implements `CloudHome`/`CloudKitOps` (whose `upload` method names
    // `UploadProgress`) and constructs `S3CloudHome`.
    #[cfg(any(test, feature = "test-utils"))]
    pub use coven::InMemoryCloudHome;
    #[cfg(feature = "oauth-providers")]
    pub use coven::{sign_in_dropbox, sign_in_google_drive, sign_in_onedrive};
    pub use coven::{
        CloudHome, CloudHomeError, CloudHomeJoinInfo, CloudKitOps, S3CloudHome, UploadProgress,
    };
}
pub mod local;
pub mod readable_path;
