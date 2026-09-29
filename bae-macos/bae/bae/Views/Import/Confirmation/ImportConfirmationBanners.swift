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
                    glyph(.warning)
                    Text("This release is already in your library")
                        .themeText(.body)
                        .foregroundStyle(StatusTone.warning.color)
                    Spacer()
                    if let albumId = libStatus.albumId {
                        Button("View in Library") {
                            onViewInLibrary(albumId)
                        }
                        .controlSize(.small)
                    }
                }
                .notice(.warning)
            }
            else if libStatus.albumInLibrary {
                HStack(spacing: ThemeSpace.related) {
                    glyph(.info)
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
                .notice(.info)
            }
        }

        importFailureBanner

        if let error {
            ErrorDetailDisclosure(error: DisplayError(line: error))
                .frame(maxWidth: .infinity, alignment: .leading)
                .notice(.danger)
        }
    }

    @ViewBuilder
    private func glyph(_ tone: StatusTone) -> some View {
        if let symbol = tone.symbol {
            Image(systemName: symbol)
                .foregroundStyle(tone.color)
        }
    }

    @ViewBuilder
    private var importFailureBanner: some View {
        switch failure {
        case .alreadyInLibrary(let albumId, let albumTitle):
            alreadyInLibrary(albumId: albumId, albumTitle: albumTitle)
        case .artistIdentityConflict(let conflict, let error):
            if let displayed = DisplayError(error) {
                artistIdentityConflict(conflict, error: displayed)
            }
        case .error(let error):
            if let displayed = DisplayError(error) {
                persistedFailure(displayed)
            }
        case nil:
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
                .notice(.danger)
        }
    }

    private func alreadyInLibrary(
        albumId: String,
        albumTitle: String
    ) -> some View {
        HStack(spacing: ThemeSpace.related) {
            glyph(.warning)
            Text(
                verbatim: coreString(
                    "ui.import.already_in_library_as",
                    albumTitle
                )
            )
            .themeText(.body)
            .foregroundStyle(StatusTone.warning.color)
            Spacer()
            Button("View in Library") { onViewInLibrary(albumId) }
                .controlSize(.small)
            Button("Retry") { onRetry() }
                .controlSize(.small)
                .disabled(!canEdit)
        }
        .notice(.warning)
    }

    private func persistedFailure(_ error: DisplayError) -> some View {
        HStack(spacing: ThemeSpace.related) {
            ErrorDetailDisclosure(error: error)
            Spacer()
            Button("Retry") { onRetry() }
                .controlSize(.small)
                .disabled(!canEdit)
        }
        .notice(.danger)
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
                    .foregroundStyle(StatusTone.warning.color)
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
                .foregroundStyle(StatusTone.warning.color)
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
                tone: .warning,
                showIcon: false
            )
        }
        .notice(.warning)
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
                failure: .error(
                    error: .Diagnostic(
                        category: .import,
                        detail: "The folder is no longer where it was"
                    )
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
