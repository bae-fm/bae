import BaeKit
import SwiftUI

/// The status banners above the confirm form: already in the library, the
/// import's failure, and the commit-time error, each only when it applies.
struct ImportConfirmationBanners: View {
    let libraryStatus: BridgeLibraryStatus?
    let importStatus: BridgeCandidateImportStatus?
    /// The error written to the candidate when committing the edit failed.
    let error: String?
    /// The last failed import of this candidate, kept across relaunches.
    let failure: BridgeError?
    let canEdit: Bool
    /// Try the failed import again.
    let onRetry: () -> Void
    let onViewInLibrary: (String) -> Void

    var body: some View {
        if let libStatus = libraryStatus {
            if libStatus.releaseInLibrary {
                Notice(
                    title: String(
                        localized: "This release is already in your library"
                    ),
                    advice: nil,
                    error: nil,
                    tone: .warning,
                    action: viewInLibrary(
                        libStatus.albums.filter(\.holdsRelease)
                    )
                )
            }
            else if libStatus.albumInLibrary {
                Notice(
                    title: String(
                        localized:
                            "Another release of this album is in your library"
                    ),
                    advice: nil,
                    error: nil,
                    tone: .info,
                    action: viewInLibrary(libStatus.albums)
                )
            }
        }

        importFailureBanner

        if let error {
            Notice(error: DisplayError(line: error))
        }
    }

    /// The way to the library album the release is in: a button for one
    /// album, a menu naming each when several hold it.
    private func viewInLibrary(_ albums: [BridgeLibraryAlbumMatch])
        -> ErrorAction?
    {
        if albums.count == 1, let album = albums.first {
            return ErrorAction("View in library") {
                onViewInLibrary(album.albumId)
            }
        }
        if albums.isEmpty {
            return nil
        }
        return ErrorAction(
            "View in library",
            choices: albums.map { album in
                ErrorAction.Choice(verbatim: album.albumTitle) {
                    onViewInLibrary(album.albumId)
                }
            }
        )
    }

    @ViewBuilder
    private var importFailureBanner: some View {
        if let failure {
            if let displayed = DisplayError(failure) {
                persistedFailure(displayed)
            }
        }
        else {
            importStatusFailure
        }
    }

    /// The running import's failure, when no stored failure speaks for it.
    @ViewBuilder
    private var importStatusFailure: some View {
        if case .error(let bridgeError) = importStatus,
            let displayed = DisplayError(bridgeError)
        {
            Notice(error: displayed)
        }
    }

    private func persistedFailure(_ error: DisplayError) -> some View {
        Notice(
            error: error,
            action: error.retryAction(isDisabled: !canEdit, perform: onRetry)
        )
    }
}
