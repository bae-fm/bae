import BaeKit
import SwiftUI

/// A library release's editor card: `lead` (the action row and the notices
/// about the draft), the cover and the identity, the Release grid, and the
/// records under a rule. Nothing unlinks a library release, so the records
/// have no Unlink.
struct MacReleaseEditorCard<Lead: View>: View {
    let session: ReleaseMetadataEditSession
    let draft: BridgeReleaseEditDraft
    var onEditCover: (() -> Void)?
    @ViewBuilder
    var lead: () -> Lead
    @Environment(Library.self)
    private var library

    var body: some View {
        VStack(
            alignment: .leading,
            spacing: ReleaseMetadataLayout.blockSpacing
        ) {
            lead()
            SessionReleaseHeader(
                session: session,
                pressingOffers: draft.pressingOffers,
                albumArtists: draft.albumArtists,
                artistSearch: library.artistSearch,
                cover: { cover },
                audioFacts: { sourceAudio },
                albumRow: {
                    MacAlbumPicker(
                        album: draft.album,
                        canEdit: true,
                        actions: session.albumActions
                    )
                }
            )
            // The catalogs that describe the release end the card, under
            // their own divider.
            if draft.hasRecords {
                MacHairline()
                MacRecordLinks(records: draft.records)
            }
        }
        .padding(.vertical, ThemeSpace.edge)
        .padding(.horizontal, ThemeSpace.section)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }

    private var cover: some View {
        Button {
            onEditCover?()
        } label: {
            Artwork(
                imageRef: draft.cover,
                pointSize: ReleaseMetadataLayout.coverSize
            )
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .disabled(onEditCover == nil)
        .accessibilityLabel(Text("Change cover"))
    }

    @ViewBuilder
    private var sourceAudio: some View {
        if let summary = draft.sourceAudio {
            SourceAudioLine(sourceAudio: summary)
        }
    }
}

/// The editable header over the form `session` holds. The header reads the
/// form's values in its own body, so a change to them redraws the header
/// alone and not the editor around it.
private struct SessionReleaseHeader<
    Cover: View,
    AudioFacts: View,
    AlbumRow: View
>: View {
    let session: ReleaseMetadataEditSession
    let pressingOffers: BridgePressingOffers
    let albumArtists: BridgeArtistField?
    let artistSearch: ArtistSearchOpener
    @ViewBuilder
    let cover: () -> Cover
    @ViewBuilder
    let audioFacts: () -> AudioFacts
    @ViewBuilder
    let albumRow: () -> AlbumRow

    var body: some View {
        ReleaseMetadataHeader(
            values: session.values,
            pressingOffers: pressingOffers,
            albumArtists: albumArtists,
            writer: session.fieldWriter,
            editingCommands: session.editingCommands,
            artistSearch: artistSearch,
            cover: cover,
            audioFacts: audioFacts,
            albumRow: albumRow
        )
    }
}
