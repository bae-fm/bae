import BaeKit
import SwiftUI

/// The release editor's form and its draft: the draft card holding `lead`
/// over the form's fields, then the album row, the folder's images where it
/// has them, the track table, and the files the release carries that no
/// track plays from.
struct ReleaseMetadataEditorContent<Lead: View>: View {
    let session: ReleaseMetadataEditSession
    /// The session's draft as its subscription sent it last.
    let draft: BridgeReleaseEditDraft
    /// What heads the draft card: the action row and the notices about it.
    @ViewBuilder
    var lead: () -> Lead
    var onEditCover: (() -> Void)?
    var onPlayTrack: ((String) -> Void)?
    /// The images of the folder the release was imported from, drawn under
    /// the album row.
    var images: [BridgeMappingImage] = []
    var onOpenImages: ((String) -> Void)?
    /// Open one of the release's documents in the viewer: the file's name,
    /// then its file id.
    let onOpenDocument: (String, String) -> Void

    @State
    private var availableWidth = ReleaseMetadataTrackColumns.minimumTableWidth
    @Environment(Library.self)
    private var library

    private var tableWidth: CGFloat {
        max(availableWidth, ReleaseMetadataTrackColumns.minimumTableWidth)
    }

    private var columns: ReleaseMetadataTrackColumns {
        .resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            VStack(
                alignment: .leading,
                spacing: ReleaseMetadataLayout.blockSpacing
            ) {
                lead()
                SessionReleaseHeader(
                    session: session,
                    albumArtistStanding: draft.albumArtistStanding,
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
                // their own divider. Nothing unlinks a library release, so no
                // Unlink beside them.
                if draft.hasRecords {
                    Rectangle()
                        .fill(Theme.hairline)
                        .frame(height: 1)
                    MacRecordLinks(records: draft.records)
                }
            }
            .padding(.vertical, ThemeSpace.edge)
            .padding(.horizontal, ThemeSpace.section)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card()
            if !images.isEmpty, let onOpenImages {
                MacImportImages(images: images, onOpen: onOpenImages)
            }
            trackTable
            if let files = draft.files {
                MacReleaseFiles(
                    rows: files.map { .file(file: $0) },
                    onOpenDocument: onOpenDocument
                )
            }
        }
        .disabled(session.isBusy)
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

    private var trackTable: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    if let sides = draft.sides {
                        ForEach(Array(sides.enumerated()), id: \.offset) {
                            index,
                            side in
                            sideRows(side, index: index)
                        }
                    }
                    else {
                        headerRow
                        Text("No tracks")
                            .themeText(.body)
                            .foregroundStyle(.secondary)
                            .frame(width: tableWidth)
                            .padding(.vertical, ThemeSpace.edge)
                    }
                }
                .frame(width: tableWidth, alignment: .leading)
            }
            .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            .onGeometryChange(for: CGFloat.self) { geometry in
                geometry.size.width
            } action: {
                availableWidth = $0
            }
        }
    }

    /// A source shared by several tracks gets a caption above them; a single
    /// track's source stays in its row.
    @ViewBuilder
    private func sideRows(_ side: BridgeReleaseEditTrackSide, index: Int)
        -> some View
    {
        if index == 0 {
            headerRow
        }
        let headerText = side.side.headerText(key: side.headerKey)
        if !headerText.isEmpty {
            sideHeader(headerText, index: index)
        }
        ForEach(Array(side.runs.enumerated()), id: \.offset) { _, run in
            switch run {
            case .track(let row):
                trackRow(row, sourceIsInCaption: false)
                    .id(row.edit.id)
            case .sheet(let source, let rows):
                sharedSourceCaption(source)
                ForEach(rows, id: \.edit.id) { row in
                    trackRow(row, sourceIsInCaption: true)
                }
            }
        }
    }

    private func sideHeader(_ text: String, index: Int) -> some View {
        MacEyebrow(verbatim: text)
            .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
            .frame(width: tableWidth, alignment: .leading)
            .padding(
                .top,
                index == 0 ? ThemeSpace.line : ThemeSpace.section
            )
            .padding(.bottom, ThemeSpace.compact)
    }

    private var headerRow: some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            MacEyebrow("Source")
                .frame(width: columns.source, alignment: .leading)
            MacEyebrow("Track")
                .frame(
                    width: ReleaseMetadataTrackColumns.track,
                    alignment: .leading
                )
            eyebrow("ui.import.mapping.column.title")
                .padding(.leading, FieldChrome.inlineHorizontalPadding)
                .frame(width: columns.title, alignment: .leading)
            eyebrow("ui.import.mapping.column.artist")
                .padding(.leading, FieldChrome.inlineHorizontalPadding)
                .frame(width: columns.artist, alignment: .leading)
            eyebrow("ui.import.slots.column.length")
                .frame(
                    width: ReleaseMetadataTrackColumns.length,
                    alignment: .leading
                )
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.top, ThemeSpace.inline)
        .padding(.bottom, ThemeSpace.compact)
    }

    private func trackRow(
        _ row: BridgeReleaseEditTrackRow,
        sourceIsInCaption: Bool
    ) -> some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            sourceCell(row.context, sourceIsInCaption: sourceIsInCaption)
            ReleaseMetadataTrackRow(
                track: row.edit,
                artistStanding: row.artistStanding,
                length: row.context.durationClock,
                listedLength: nil,
                columns: columns,
                editingCommands: session.editingCommands,
                position: row.context.positionText,
                artistSearch: library.artistSearch,
                onChange: { await session.updateTrack($0) }
            )
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.vertical, ThemeSpace.compact)
        .frame(minHeight: 40)
        .overlay(alignment: .top) {
            Rectangle().fill(Theme.hairline).frame(height: 1)
        }
    }

    private func sourceCell(
        _ context: BridgeReleaseEditTrackContext,
        sourceIsInCaption: Bool
    ) -> some View {
        HStack(spacing: ThemeSpace.related) {
            Button {
                onPlayTrack?(context.trackId)
            } label: {
                Image(systemName: "play.fill")
                    .themeIcon(.small)
            }
            .buttonStyle(.accentPlain)
            .foregroundStyle(.secondary)
            .opacity(onPlayTrack == nil ? 0 : 1)
            .allowsHitTesting(onPlayTrack != nil)
            if sourceIsInCaption {
                TrackStartText(start: context.startClock)
            }
            else {
                let name = context.source.name
                Text(verbatim: name)
                    .themeText(.mono)
                    .cutLine(.fileName)
                    .help(name)
            }
        }
        .frame(width: columns.source, alignment: .leading)
    }

    private func sharedSourceCaption(
        _ source: BridgeReleaseEditTrackSource
    ) -> some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "list.bullet.rectangle")
                .foregroundStyle(.tertiary)
            Text(source.name)
                .themeText(.mono)
                .cutLine(.fileName)
                .help(source.name)
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.top, ThemeSpace.related)
        .padding(.bottom, ThemeSpace.compact)
        .frame(width: tableWidth, alignment: .leading)
    }

    private func eyebrow(_ key: String) -> some View {
        MacEyebrow(verbatim: localizedCoreString(key))
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
    let albumArtistStanding: BridgeArtistFieldStanding?
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
            albumArtistStanding: albumArtistStanding,
            writer: session.fieldWriter,
            editingCommands: session.editingCommands,
            artistSearch: artistSearch,
            cover: cover,
            audioFacts: audioFacts,
            albumRow: albumRow
        )
    }
}
