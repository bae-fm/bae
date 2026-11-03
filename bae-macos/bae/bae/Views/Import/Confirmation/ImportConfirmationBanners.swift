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
    /// The person's answer, including returning to the release picker.
    let onChooseArtistIdentity: (BridgeArtistIdentityConflictChoice) -> Void
    /// The person's answer to a label conflict, including returning to the
    /// release picker.
    let onChooseLabelIdentity: (BridgeLabelIdentityConflictChoice) -> Void
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
                    viewInLibrary(libStatus.albums.filter(\.holdsRelease))
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
                    viewInLibrary(libStatus.albums)
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

    /// The way to the library album the release is in: a button for one
    /// album, a menu naming each when several hold it.
    @ViewBuilder
    private func viewInLibrary(_ albums: [BridgeLibraryAlbumMatch]) -> some View
    {
        if albums.count == 1, let album = albums.first {
            Button("View in Library") { onViewInLibrary(album.albumId) }
                .controlSize(.small)
        }
        else if !albums.isEmpty {
            Menu("View in Library") {
                ForEach(albums, id: \.albumId) { album in
                    Button(album.albumTitle) { onViewInLibrary(album.albumId) }
                }
            }
            .controlSize(.small)
            .fixedSize()
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
        case .artistIdentityConflict(let conflict, let error):
            if let displayed = DisplayError(error) {
                artistIdentityConflict(conflict, error: displayed)
            }
        case .labelIdentityConflict(let conflict, let error):
            if let displayed = DisplayError(error) {
                labelIdentityConflict(conflict, error: displayed)
            }
        case .other(let error):
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
        let answer = onChooseArtistIdentity
        return identityConflict(
            kind: "artist",
            symbol: "person.2.badge.gearshape.fill",
            incomingName: conflict.incomingArtistName,
            error: error,
            choices: [
                IdentityChoice(key: "same_artist") { answer(.sameArtist) },
                IdentityChoice(
                    key: "discogs_artist",
                    existingName: conflict.discogsArtist.name
                ) { answer(.discogsArtist) },
                IdentityChoice(
                    key: "musicbrainz_artist",
                    existingName: conflict.musicbrainzArtist.name
                ) { answer(.musicBrainzArtist) },
                IdentityChoice(key: "neither") { answer(.neither) },
                IdentityChoice(key: "change_match") { answer(.changeMatch) },
            ]
        )
    }

    private func labelIdentityConflict(
        _ conflict: BridgeLabelIdentityConflict,
        error: DisplayError
    ) -> some View {
        let answer = onChooseLabelIdentity
        return identityConflict(
            kind: "label",
            symbol: "building.2.fill",
            incomingName: conflict.incomingLabelName,
            error: error,
            choices: [
                IdentityChoice(key: "same_label") { answer(.sameLabel) },
                IdentityChoice(
                    key: "discogs_label",
                    existingName: conflict.discogsLabel.name
                ) { answer(.discogsLabel) },
                IdentityChoice(
                    key: "musicbrainz_label",
                    existingName: conflict.musicbrainzLabel.name
                ) { answer(.musicBrainzLabel) },
                IdentityChoice(key: "neither") { answer(.neither) },
                IdentityChoice(key: "change_match") { answer(.changeMatch) },
            ]
        )
    }

    /// One answer to an identity conflict: its string key under the
    /// conflict's kind, the library entry it names, and what it sends.
    private struct IdentityChoice {
        let key: String
        var existingName: String?
        let choose: () -> Void
    }

    /// The banner for an incoming artist or label whose two catalog IDs name
    /// two library entries: its name, why it stopped, and the five answers.
    private func identityConflict(
        kind: String,
        symbol: String,
        incomingName: String,
        error: DisplayError,
        choices: [IdentityChoice]
    ) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            HStack(
                alignment: .firstTextBaseline,
                spacing: ThemeSpace.related
            ) {
                Image(systemName: symbol)
                    .foregroundStyle(StatusTone.warning.color)
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text(incomingName)
                        .themeText(.strong)
                    Text(
                        verbatim: localizedCoreString(
                            "ui.import.\(kind)_identity_conflict.explanation"
                        )
                    )
                }
                .themeText(.body)
                .foregroundStyle(StatusTone.warning.color)
            }
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                ForEach(choices, id: \.key) { choice in
                    Button(action: choice.choose) {
                        VStack(
                            alignment: .leading,
                            spacing: ThemeSpace.hairline
                        ) {
                            Text(
                                verbatim: localizedCoreString(
                                    "ui.import.\(kind)_identity_conflict.\(choice.key)"
                                )
                            )
                            if let name = choice.existingName {
                                Text(verbatim: name)
                                    .themeText(.detail)
                            }
                        }
                    }
                    .accessibilityIdentifier("\(kind)-identity-\(choice.key)")
                }
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
                failure: .other(
                    error: .Diagnostic(
                        category: .import,
                        detail: "The folder is no longer where it was"
                    )
                ),
                canEdit: true,
                onRetry: {},
                onChooseArtistIdentity: { _ in },
                onChooseLabelIdentity: { _ in },
                onViewInLibrary: { _ in },
            )
        }
        .padding()
        .frame(width: 480)
        .windowBackground()
    }
#endif
