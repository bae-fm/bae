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
    let failure: BridgeImportFailure?
    let canEdit: Bool
    /// Try the failed import again.
    let onRetry: () -> Void
    /// Keep the named library artist and absorb the other row.
    let onMergeArtists: (String) -> Void
    let onViewInLibrary: (String) -> Void

    var body: some View {
        if let libStatus = libraryStatus {
            if libStatus.releaseInLibrary {
                HStack(spacing: ThemeSpace.related) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(NoticeTone.warning.tint)
                    Text("This release is already in your library")
                        .themeText(.body)
                        .foregroundStyle(NoticeTone.warning.tint)
                    Spacer()
                    if let albumId = libStatus.albumId {
                        Button("View in Library") {
                            onViewInLibrary(albumId)
                        }
                        .controlSize(.small)
                    }
                }
                .padding(ThemeSpace.group)
                .noticeBackground(.warning)
            }
            else if libStatus.albumInLibrary {
                HStack(spacing: ThemeSpace.related) {
                    Image(systemName: "info.circle.fill")
                        .foregroundStyle(NoticeTone.info.tint)
                    Text(
                        "Another release of this album is in your library"
                    )
                    .themeText(.body)
                    Spacer()
                    if let albumId = libStatus.albumId {
                        Button("View in Library") {
                            onViewInLibrary(albumId)
                        }
                        .controlSize(.small)
                    }
                }
                .padding(ThemeSpace.group)
                .noticeBackground(.info)
            }
        }

        importFailureBanner

        if let error {
            HStack(spacing: ThemeSpace.related) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(NoticeTone.error.tint)
                Text(error)
                    .themeText(.body)
                    .foregroundStyle(NoticeTone.error.tint)
            }
            .padding(ThemeSpace.group)
            .noticeBackground(.error)
        }
    }

    @ViewBuilder
    private var importFailureBanner: some View {
        if let failure, let displayed = DisplayError(failure.error) {
            if let conflict = failure.artistIdentityConflict {
                artistIdentityConflict(conflict, error: displayed)
            }
            else {
                persistedFailure(displayed)
            }
        }
        else if case .error(let bridgeError) = importStatus,
            let displayed = DisplayError(bridgeError)
        {
            ErrorDetailDisclosure(error: displayed)
                .padding(ThemeSpace.group)
                .noticeBackground(.error)
        }
    }

    private func persistedFailure(_ error: DisplayError) -> some View {
        HStack(spacing: ThemeSpace.related) {
            ErrorDetailDisclosure(error: error)
            Spacer()
            Button("Retry") { onRetry() }
                .controlSize(.small)
                .disabled(!canEdit)
        }
        .padding(ThemeSpace.group)
        .noticeBackground(.error)
    }

    private func artistIdentityConflict(
        _ conflict: BridgeArtistIdentityConflict,
        error: DisplayError
    ) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            HStack(
                alignment: .firstTextBaseline,
                spacing: ThemeSpace.related
            ) {
                Image(systemName: "person.2.badge.gearshape.fill")
                    .foregroundStyle(NoticeTone.warning.tint)
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text(conflict.incomingArtistName)
                        .themeText(.strong)
                    Text(
                        verbatim: coreString(
                            "ui.import.artist_identity_conflict.explanation"
                        )
                    )
                }
                .themeText(.body)
                .foregroundStyle(NoticeTone.warning.tint)
            }
            HStack(spacing: ThemeSpace.related) {
                artistChoiceButton(
                    titleKey:
                        "ui.import.artist_identity_conflict.keep_discogs",
                    artist: conflict.discogsArtist
                )
                artistChoiceButton(
                    titleKey:
                        "ui.import.artist_identity_conflict.keep_musicbrainz",
                    artist: conflict.musicbrainzArtist
                )
            }
            .controlSize(.small)
            .disabled(!canEdit)
            ErrorDetailDisclosure(
                error: error,
                tint: NoticeTone.warning.tint,
                showIcon: false
            )
        }
        .padding(ThemeSpace.group)
        .noticeBackground(.warning)
    }

    private func artistChoiceButton(
        titleKey: String,
        artist: BridgeExistingArtist
    ) -> some View {
        Button {
            onMergeArtists(artist.artistId)
        } label: {
            VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                Text(verbatim: coreString(titleKey))
                Text(verbatim: artist.name)
                    .themeText(.detail)
            }
        }
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
                    albumTitle: "Album Title",
                    albumId: "preview-album"
                ),
                importStatus: nil,
                error: "Couldn't shape the edit: missing album title",
                failure: BridgeImportFailure(
                    error: .Diagnostic(
                        category: .import,
                        detail: "The folder is no longer where it was"
                    ),
                    artistIdentityConflict: nil
                ),
                canEdit: true,
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
