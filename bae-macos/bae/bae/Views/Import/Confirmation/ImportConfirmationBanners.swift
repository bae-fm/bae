import BaeKit
import SwiftUI

/// The stacked status banners above the confirm form: an already-in-library
/// warning (release or album), a track-count-mismatch warning, the import
/// pipeline's error disclosure, and the commit-time error line. Each is
/// conditional on its input, so an all-clear candidate renders nothing.
struct ImportConfirmationBanners: View {
    let libraryStatus: BridgeLibraryStatus?
    let trackCountMismatch: Bool
    let expectedTrackCount: UInt32
    let importStatus: BridgeCandidateImportStatus?
    /// Commit-time error written to the candidate (invalid edit shape, a failed
    /// `start_import` dispatch). Distinct from the `importStatus`-derived error,
    /// which the import pipeline emits once an import is under way.
    let error: String?
    let onViewInLibrary: (String) -> Void

    var body: some View {
        if let libStatus = libraryStatus {
            if libStatus.releaseInLibrary {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text("This release is already in your library")
                        .font(.callout)
                        .foregroundStyle(.orange)
                    Spacer()
                    if let albumId = libStatus.albumId {
                        Button("View in Library") {
                            onViewInLibrary(albumId)
                        }
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(Color.orange.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            else if libStatus.albumInLibrary {
                HStack(spacing: 8) {
                    Image(systemName: "info.circle.fill")
                        .foregroundStyle(.blue)
                    Text(
                        "Another release of this album is in your library"
                    )
                    .font(.callout)
                    Spacer()
                    if let albumId = libStatus.albumId {
                        Button("View in Library") {
                            onViewInLibrary(albumId)
                        }
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(Color.blue.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }
        }

        if trackCountMismatch {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(
                    "Track count mismatch: release has \(Int(expectedTrackCount)) tracks but local files don't match"
                )
                .font(.callout)
                .foregroundStyle(.orange)
            }
            .padding(10)
            .background(Color.orange.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }

        if case .error(let bridgeError) = importStatus,
            let displayed = DisplayError(bridgeError)
        {
            ErrorDetailDisclosure(error: displayed)
                .padding(10)
                .background(Color.red.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
        }

        if let error {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                Text(error)
                    .font(.callout)
                    .foregroundStyle(.red)
            }
            .padding(10)
            .background(Color.red.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }
    }
}

#if DEBUG
    #Preview("Confirmation banners") {
        VStack(spacing: 12) {
            ImportConfirmationBanners(
                libraryStatus: BridgeLibraryStatus(
                    releaseId: "rel-123",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: "preview-album"
                ),
                trackCountMismatch: true,
                expectedTrackCount: 9,
                importStatus: nil,
                error: "Couldn't shape the edit: missing album title",
                onViewInLibrary: { _ in },
            )
        }
        .padding()
        .frame(width: 480)
        .windowBackground()
    }
#endif
