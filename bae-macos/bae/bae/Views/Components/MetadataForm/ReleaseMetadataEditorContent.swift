import BaeKit
import OSLog
import SwiftUI

/// The release editor body shared by Import Done and the Library modal.
struct ReleaseMetadataEditorContent: View {
    private static let logger = Logger.bae("ReleaseMetadataEditor")

    let session: ReleaseMetadataEditSession
    var onEditCover: (() -> Void)?
    var onPlayTrack: ((String) -> Void)?

    @State
    private var availableWidth = ReleaseMetadataTrackColumns.minimumTableWidth
    /// What the library holds for the form's credits, read again whenever the
    /// form changes.
    @State
    private var artistResolutions: [BridgeResolvedCredit] = []
    @Environment(Library.self)
    private var library

    private var tableWidth: CGFloat {
        max(availableWidth, ReleaseMetadataTrackColumns.minimumTableWidth)
    }

    private var columns: ReleaseMetadataTrackColumns {
        .resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.section) {
            ReleaseMetadataHeader(
                values: session.form,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                cover: { cover },
                audioFacts: { sourceAudio }
            )
            trackTable
        }
        .disabled(session.isBusy)
        .environment(\.artistResolutions, artistResolutions)
        .task(id: session.form) {
            do {
                artistResolutions = try await library.resolveReleaseEditCredits(
                    session.form
                )
            }
            catch is CancellationError {}
            catch {
                // No badge beats a stale one; the save resolves every credit
                // itself.
                artistResolutions = []
                Self.logger.error(
                    "Could not read the form's artist credits: \(error)"
                )
            }
        }
    }

    private var cover: some View {
        Button {
            onEditCover?()
        } label: {
            ImageView(
                imageRef: session.cover,
                pointSize: ReleaseMetadataLayout.coverSize
            )
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            .overlay(alignment: .topTrailing) {
                Image(systemName: "pencil")
                    .themeIcon(.badge)
                    .foregroundStyle(Theme.onFill)
                    .padding(ThemeSpace.inline)
                    .background(Theme.scrim)
                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
                    .padding(ThemeSpace.inline)
                    .opacity(onEditCover == nil ? 0 : 1)
                    .allowsHitTesting(false)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(onEditCover == nil)
        .accessibilityLabel(Text("Change Cover"))
    }

    @ViewBuilder
    private var sourceAudio: some View {
        if let summary = session.display.sourceAudio {
            ReleaseSourceAudioSummaryView(sourceAudio: summary)
        }
    }

    private var trackTable: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            FormSectionHeader(title: String(localized: "Tracks"), ruled: true)
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
        if !side.headerText.isEmpty {
            sideHeader(side.headerText, index: index)
        }
        ForEach(Array(side.groups.enumerated()), id: \.element.id) {
            groupIndex,
            group in
            let captionSource =
                group.tracks.count > 1 ? group.sharedSource : nil
            if let source = captionSource {
                sharedSourceCaption(source)
            }
            if index == 0 && groupIndex == 0 {
                headerRow
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
                    alignment: .trailing
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
                duration: releaseDurationText(item.context.durationMs),
                durationDiverges: false,
                columns: columns,
                editingCommands: session.editingCommands,
                onChange: { session.updateTrack($0) }
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
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .opacity(onPlayTrack == nil ? 0 : 1)
            .allowsHitTesting(onPlayTrack != nil)
            if !sourceIsInCaption {
                let name = item.context.sources.map(\.name)
                    .joined(separator: " + ")
                Text(verbatim: name)
                    .themeText(.mono)
                    .lineLimit(1)
                    .truncationMode(.middle)
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
                .lineLimit(1)
                .truncationMode(.middle)
                .help(source.name)
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.top, ThemeSpace.related)
        .padding(.bottom, ThemeSpace.compact)
        .frame(width: tableWidth, alignment: .leading)
    }

    private func eyebrow(_ key: String) -> some View {
        Eyebrow(verbatim: coreString(key))
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
            .accessibilityLabel(coreString("core.audio.label"))
            .accessibilityValue(sourceAudio.text)
    }
}
