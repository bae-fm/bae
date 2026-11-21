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
            MacReleaseEditorCard(
                session: session,
                draft: draft,
                onEditCover: onEditCover,
                lead: lead
            )
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
                            .foregroundStyle(Theme.macTextSecondary)
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

    /// The tracks a CUE sheet carves get its caption above them; a track
    /// that plays a whole file names it in its row.
    @ViewBuilder
    private func sideRows(_ side: BridgeTrackTableSide, index: Int)
        -> some View
    {
        if index == 0 {
            headerRow
        }
        let headerText = side.side.headingText
        if !headerText.isEmpty {
            sideHeader(headerText, index: index)
        }
        ForEach(Array(side.runs.enumerated()), id: \.offset) { _, run in
            if case .sheet(let sheet, _) = run {
                ReleaseSheetCaption(sheet: sheet, actions: sheetActions)
                    .padding(
                        .horizontal,
                        ReleaseMetadataTrackColumns.rowPadding
                    )
                    .padding(.top, ThemeSpace.related)
                    .padding(.bottom, ThemeSpace.compact)
                    .frame(width: tableWidth, alignment: .leading)
            }
            ForEach(run.rows, id: \.track.id, content: trackRow)
        }
    }

    /// The caption's actions on the session's form: a side or disc picked
    /// for a sheet, or an audio file for one of its lines. The caption
    /// offers neither *Ignored* nor binding no audio, since the editor keeps
    /// every track it opened on.
    private var sheetActions: SheetCaptionActions {
        SheetCaptionActions(
            open: onOpenDocument,
            place: { sheetId, place in
                if case .on(let side) = place {
                    session.pickSheetSide(sheetId, side: side)
                }
            },
            bind: { sheetId, fileReference, audioFileId in
                if let audioFileId {
                    session.pickSheetFile(
                        sheetId,
                        fileReference: fileReference,
                        audioFileId: audioFileId
                    )
                }
            }
        )
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

    private func trackRow(_ row: BridgeTrackTableRow) -> some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            sourceCell(trackId: row.track.id, source: row.source)
            ReleaseMetadataTrackRow(
                track: row.track,
                artists: row.artists,
                length: row.length,
                listedLength: row.listedLength,
                columns: columns,
                editingCommands: session.editingCommands,
                position: row.position,
                artistSearch: library.artistSearch,
                onChange: { await session.updateTrack($0) }
            )
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.vertical, ThemeSpace.compact)
        .frame(minHeight: 40)
        .overlay(alignment: .top) {
            MacHairline()
        }
    }

    /// The row's play button, which plays the release from the track
    /// `trackId`, and what the row's source cell names.
    private func sourceCell(
        trackId: String,
        source: BridgeTrackSource
    ) -> some View {
        HStack(spacing: ThemeSpace.related) {
            Button {
                onPlayTrack?(trackId)
            } label: {
                Image(systemName: "play.fill")
                    .themeIcon(.small)
            }
            .buttonStyle(.accentPlain)
            .foregroundStyle(Theme.macTextSecondary)
            .opacity(onPlayTrack == nil ? 0 : 1)
            .allowsHitTesting(onPlayTrack != nil)
            switch source {
            case .sheetSlice(let start):
                TrackStartText(start: start)
            case .file(let name):
                Text(verbatim: name)
                    .themeText(.mono)
                    .cutLine(.fileName)
                    .help(name)
            }
        }
        .frame(width: columns.source, alignment: .leading)
    }

    private func eyebrow(_ key: String) -> some View {
        MacEyebrow(verbatim: localizedCoreString(key))
    }
}
