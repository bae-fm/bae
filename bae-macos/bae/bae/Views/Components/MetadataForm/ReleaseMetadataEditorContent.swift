import BaeKit
import SwiftUI

/// The release editor's draft: the draft card holding `lead` over the
/// release's fields, then the album row, the folder's images where it has
/// them, the track table, and the files the release carries that no track
/// plays from.
struct ReleaseMetadataEditorContent<Lead: View>: View {
    let session: ReleaseMetadataEditSession
    /// What heads the draft card: the action row and the notices about it.
    @ViewBuilder
    var lead: () -> Lead
    var onEditCover: (() -> Void)?
    /// Show a library album the album row names.
    let onViewAlbum: (String) -> Void
    var onPlayTrack: ((String) -> Void)?
    /// The images of the folder the release was imported from, drawn under
    /// the album row.
    var images: [BridgeMappingImage] = []
    var onOpenImages: (([BridgeMappingImage], String) -> Void)?
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
                ReleaseMetadataHeader(
                    values: session.draft.edit,
                    writer: session.fieldWriter,
                    editingCommands: session.editingCommands,
                    artistSearch: library.artistSearch,
                    cover: { cover },
                    audioFacts: { sourceAudio }
                )
                DraftAlbumRow(
                    album: session.draft.album,
                    canEdit: true,
                    actions: session.albumActions(viewInLibrary: onViewAlbum)
                )
            }
            .padding(.vertical, ThemeSpace.edge)
            .padding(.horizontal, ThemeSpace.section)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card()
            if !images.isEmpty, let onOpenImages {
                MacImportImages(images: images, onOpen: onOpenImages)
            }
            trackTable
            if !session.draft.display.files.isEmpty {
                ReleaseFilesView(
                    rows: session.draft.display.files.map { .file(file: $0) },
                    onOpenDocument: onOpenDocument
                )
            }
        }
        .disabled(session.isBusy)
        .environment(\.artistResolutions, session.draft.artistResolutions)
    }

    private var cover: some View {
        Button {
            onEditCover?()
        } label: {
            ImageView(
                imageRef: session.draft.cover,
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
        if let summary = session.draft.display.sourceAudio {
            ReleaseSourceAudioSummaryView(sourceAudio: summary)
        }
    }

    private var trackTable: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    if session.trackSides.isEmpty {
                        headerRow
                        Text("No tracks")
                            .themeText(.body)
                            .foregroundStyle(.secondary)
                            .frame(width: tableWidth)
                            .padding(.vertical, ThemeSpace.edge)
                    }
                    else {
                        ForEach(
                            Array(session.trackSides.enumerated()),
                            id: \.element.id
                        ) { index, side in
                            sideRows(side, index: index)
                        }
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
    private func sideRows(_ side: ReleaseMetadataTrackSide, index: Int)
        -> some View
    {
        if index == 0 {
            headerRow
        }
        if !side.headerText.isEmpty {
            sideHeader(side.headerText, index: index)
        }
        ForEach(Array(side.groups.enumerated()), id: \.element.id) {
            _,
            group in
            let captionSource = group.sharedSource
            if let source = captionSource {
                sharedSourceCaption(source)
            }
            ForEach(group.tracks) { item in
                trackRow(
                    item,
                    sourceIsInCaption: captionSource != nil
                )
            }
        }
    }

    private func sideHeader(_ text: String, index: Int) -> some View {
        Eyebrow(verbatim: text)
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
            Eyebrow("Source")
                .frame(width: columns.source, alignment: .leading)
            Eyebrow("Track")
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
        _ item: ReleaseMetadataTrackItem,
        sourceIsInCaption: Bool
    ) -> some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            sourceCell(item, sourceIsInCaption: sourceIsInCaption)
            ReleaseMetadataTrackRow(
                track: item.track,
                duration: DurationClock.label(item.context.durationClock),
                durationDiverges: false,
                columns: columns,
                editingCommands: session.editingCommands,
                position: item.context.positionText,
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
        _ item: ReleaseMetadataTrackItem,
        sourceIsInCaption: Bool
    ) -> some View {
        HStack(spacing: ThemeSpace.related) {
            Button {
                onPlayTrack?(item.context.trackId)
            } label: {
                Image(systemName: "play.fill")
                    .themeIcon(.small)
            }
            .buttonStyle(.accentPlain)
            .foregroundStyle(.secondary)
            .opacity(onPlayTrack == nil ? 0 : 1)
            .allowsHitTesting(onPlayTrack != nil)
            if !sourceIsInCaption {
                let name = item.context.source.name
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
        Eyebrow(verbatim: localizedCoreString(key))
    }
}

struct ReleaseSourceAudioSummaryView: View {
    let sourceAudio: BridgeSourceAudioSummary

    var body: some View {
        Text(sourceAudio.text)
            .themeText(.detail)
            .foregroundStyle(.tertiary)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityLabel(localizedCoreString("core.audio.label"))
            .accessibilityValue(sourceAudio.text)
    }
}
