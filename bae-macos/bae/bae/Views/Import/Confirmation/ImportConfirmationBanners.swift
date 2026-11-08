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
                HStack(spacing: ThemeSpace.related) {
                    StatusGlyph(.warning)
                    Text("This release is already in your library")
                        .themeText(.body)
                    Spacer()
                    viewInLibrary(libStatus.albums.filter(\.holdsRelease))
                }
                .notice(.warning)
            }
            else if libStatus.albumInLibrary {
                HStack(spacing: ThemeSpace.related) {
                    StatusGlyph(.info)
                    Text(
                        "Another release of this album is in your library"
                    )
                    .themeText(.body)
                    Spacer()
                    viewInLibrary(libStatus.albums)
                }
                .notice(.info)
            }
        }

        importFailureBanner

        if let error {
            ErrorDetailDisclosure(error: DisplayError(line: error))
        }
    }

    /// The way to the library album the release is in: a button for one
    /// album, a menu naming each when several hold it.
    @ViewBuilder
    private func viewInLibrary(_ albums: [BridgeLibraryAlbumMatch]) -> some View
    {
        if albums.count == 1, let album = albums.first {
            Button("View in library") { onViewInLibrary(album.albumId) }
                .controlSize(.small)
        }
        else if !albums.isEmpty {
            Menu("View in library") {
                ForEach(albums, id: \.albumId) { album in
                    Button(album.albumTitle) { onViewInLibrary(album.albumId) }
                }
            }
            .controlSize(.small)
            .fixedSize()
        }
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
            ErrorDetailDisclosure(error: displayed)
        }
    }

    private func persistedFailure(_ error: DisplayError) -> some View {
        ErrorDetailDisclosure(
            error: error,
            action: ErrorAction(isDisabled: !canEdit, perform: onRetry)
        )
    }
}

#if DEBUG
    #Preview("Confirmation banners") {
        VStack(spacing: ThemeSpace.group) {
            ImportConfirmationBanners(
                libraryStatus: BridgeLibraryStatus(
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
                ),
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
#endif
