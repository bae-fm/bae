//! The Storage sheet's live read of a release's files on [`AppServices`].

use super::LiveReleaseFiles;
use super::*;
use bae_views::ReleaseFilesSnapshot;

/// The files of the release the subscription is set to, in the order it
/// asks for, as they change: each value answers the request set last.
/// Another release or order is a new request on the same read.
pub fn subscribe_release_files(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> LiveReleaseFiles {
    let views = library_views.clone();
    follow_request(
        library,
        "release files",
        move |request| views.subscribe_release_files(request),
        |request, files| ReleaseFilesSnapshot {
            request: request.clone(),
            files: files.map(|files| {
                files.into_iter().map(bae_views::album_detail::FileDetail::from_raw).collect()
            }),
        },
    )
}
