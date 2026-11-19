import BaeKit
import SwiftUI

/// The album identity drawn as a heading that is editable in place.
struct MacReleaseIdentity<AudioFacts: View, AlbumRow: View>: View {
    let values: BridgeRawReleaseEdit
    let albumArtistStanding: BridgeArtistFieldStanding?
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    let artistSearch: ArtistSearchOpener
    @ViewBuilder
    let audioFacts: () -> AudioFacts
    @ViewBuilder
    let albumRow: () -> AlbumRow

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            CommittedTextField(
                placeholder: String(localized: "Album title"),
                value: values.albumTitle,
                name: String(localized: "Album title"),
                chrome: .inline,
                font: ThemeText.title.nsFont,
                editingCommands: editingCommands,
                onCommit: { await writer.setField(.albumTitle, $0) },
            )
            MacAlbumArtistField(
                assignments: values.albumArtistAssignments,
                standing: albumArtistStanding,
                placeholder: String(localized: "Album artist"),
                artistSearch: artistSearch,
                onChange: { assignments in
                    Task { await writer.setAlbumArtists(assignments) }
                },
            )
            .themeText(.body)
            .foregroundStyle(.secondary)
            .modifier(FieldChrome(focused: false, style: .inline))
            // The album's original year; the pressing's year is under Release.
            CommittedTextField(
                placeholder: String(localized: "Album year"),
                value: values.albumYear,
                name: String(localized: "Album year"),
                chrome: .inline,
                font: ThemeText.body.nsFont,
                textColor: .tertiaryLabelColor,
                editingCommands: editingCommands,
                onCommit: { await writer.setField(.albumYear, $0) },
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
