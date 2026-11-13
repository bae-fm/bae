import BaeKit
import Foundation
import SwiftUI

@testable import bae

extension ImportConfirmationBanners {
    /// A release already in the library.
    static let previewLibraryStatus = BridgeLibraryStatus(
        releaseId: "rel-123",
        releaseInLibrary: true,
        albumInLibrary: true,
        albums: [
            BridgeLibraryAlbumMatch(
                albumId: "preview-album",
                albumTitle: "Album Title",
                holdsRelease: true
            )
        ]
    )

    /// The banners as the confirm form shows them: the release in the
    /// library, the failed import with Retry, and the commit error.
    static var preview: some View {
        VStack(spacing: ThemeSpace.group) {
            ImportConfirmationBanners(
                libraryStatus: previewLibraryStatus,
                importStatus: nil,
                error: "Couldn't shape the edit: missing album title",
                failure: .Diagnostic(
                    category: .import,
                    detail: "The folder is no longer where it was"
                ),
                canEdit: true,
                onRetry: {},
                onViewInLibrary: { _ in },
            )
        }
        .padding()
        .frame(width: 480)
        .windowBackground()
    }
}
