import BaeKit
import SwiftUI

/// The release's cover beside its identity, drawn as a heading that is
/// editable in place.
struct MacReleaseIdentity<Cover: View, AudioFacts: View, AlbumRow: View>: View {
    let values: BridgeRawReleaseEdit
    let albumArtists: BridgeArtistField?
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    let artistSearch: ArtistSearchOpener
    @ViewBuilder
    let cover: () -> Cover
    @ViewBuilder
    let audioFacts: () -> AudioFacts
    @ViewBuilder
    let albumRow: () -> AlbumRow

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.group) {
            // Clipped here so a non-square cover never spills over the
            // identity column.
            cover()
                .frame(
                    width: ReleaseMetadataLayout.coverSize,
                    height: ReleaseMetadataLayout.coverSize
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            identity
        }
    }

    private var identity: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            MacInlineField(
                placeholder: String(localized: "Album title"),
                value: values.albumTitle,
                name: String(localized: "Album title"),
                font: ThemeText.title.nsFont,
                editingCommands: editingCommands,
                onCommit: { await writer.setField(.albumTitle, $0) }
            )
            MacAlbumArtistField(
                field: albumArtists,
                placeholder: String(localized: "Album artist"),
                artistSearch: artistSearch,
                onChange: { assignments in
                    Task { await writer.setAlbumArtists(assignments) }
                },
            )
            .themeText(.body)
            .foregroundStyle(Theme.macTextSecondary)
            .modifier(FieldChrome(focused: false, style: .inline))
            // The album's original year; the pressing's year is under Release.
            MacInlineField(
                placeholder: String(localized: "Album year"),
                value: values.albumYear,
                name: String(localized: "Album year"),
                font: ThemeText.body.nsFont,
                textColor: NSColor(Theme.textTertiary),
                editingCommands: editingCommands,
                onCommit: { await writer.setField(.albumYear, $0) }
            )
            audioFacts()
                .padding(.horizontal, FieldChrome.inlineHorizontalPadding)
            albumRow()
                .padding(.horizontal, FieldChrome.inlineHorizontalPadding)
        }
        .padding(.leading, -FieldChrome.inlineHorizontalPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
