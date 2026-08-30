import BaeKit
import SwiftUI

/// The stacked status banners above the confirm form: an already-in-library
/// warning (release or album), the import pipeline's error disclosure, and the
/// commit-time error line. Each is conditional on its input, so an all-clear
/// candidate renders nothing.
///
/// A source tracklist that disagrees with the folder's audio is not one of
/// these: it is stated on the track slot it belongs to.
struct ImportConfirmationBanners: View {
    let libraryStatus: BridgeLibraryStatus?
    let importStatus: BridgeTriageImportStatus?
    /// Commit-time error written to the candidate (invalid edit shape, a failed
    /// `start_import` dispatch). Distinct from the `importStatus`-derived error,
    /// which the candidate's row carries once an import has failed.
    let error: String?
    /// The last import of this candidate that failed, as it survives a
    /// relaunch. Shown when nothing is running for the candidate — while an
    /// import is under way, its own status is the current answer.
    let failure: BridgeImportFailure?
    /// Try the failed import again.
    let onRetry: () -> Void
    /// Keep the named library artist and absorb the other row.
    let onMergeArtists: (String) -> Void
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

        if case .error(let bridgeError) = importStatus,
            let displayed = DisplayError(bridgeError)
        {
            ErrorDetailDisclosure(error: displayed)
                .padding(10)
                .background(Color.red.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
        }

        if importStatus == nil, let failure {
            switch failure {
            case .error(let error, _):
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                    Text(error)
                        .font(.callout)
                        .foregroundStyle(.red)
                    Spacer()
                    Button("Retry") { onRetry() }
                        .controlSize(.small)
                }
                .padding(10)
                .background(Color.red.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            case .artistIdentityConflict(_, _, let conflict):
                artistIdentityConflict(conflict)
            }
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

    private func artistIdentityConflict(
        _ conflict: BridgeArtistIdentityConflict
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "person.2.badge.gearshape.fill")
                    .foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 2) {
                    Text(conflict.incomingArtistName)
                        .fontWeight(.semibold)
                    Text(
                        "This source identity is linked to two artists in your library. Choose the record to keep; its albums, tracks, credits, and source links will be combined with the other record."
                    )
                }
                .font(.callout)
                .foregroundStyle(.orange)
            }
            HStack(spacing: 8) {
                Button {
                    onMergeArtists(conflict.discogsArtist.artistId)
                } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Keep Discogs Artist")
                        Text(conflict.discogsArtist.name)
                            .font(.caption)
                    }
                }
                Button {
                    onMergeArtists(conflict.musicbrainzArtist.artistId)
                } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Keep MusicBrainz Artist")
                        Text(conflict.musicbrainzArtist.name)
                            .font(.caption)
                    }
                }
            }
            .controlSize(.small)
        }
        .padding(10)
        .background(Color.orange.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
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
                importStatus: nil,
                error: "Couldn't shape the edit: missing album title",
                failure: BridgeImportFailure.error(
                    error: "The folder is no longer where it was",
                    failedAt: "2026-08-23T04:00:00Z"
                ),
                onRetry: {},
                onMergeArtists: { _ in },
                onViewInLibrary: { _ in },
            )
        }
        .padding()
        .frame(width: 480)
        .windowBackground()
    }
#endif
