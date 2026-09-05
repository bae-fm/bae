import BaeKit
import SwiftUI

/// Shared persisted-release editor body. Import Done and the Library modal
/// provide their shell and actions; this renders one header and track table.
struct ReleaseMetadataEditorContent: View {
    let session: ReleaseMetadataEditSession
    var onEditCover: (() -> Void)?
    var onPlayTrack: ((Int) -> Void)?

    @State
    private var availableWidth = ReleaseMetadataTrackColumns.minimumTableWidth

    private var tableWidth: CGFloat {
        max(availableWidth, ReleaseMetadataTrackColumns.minimumTableWidth)
    }

    private var columns: ReleaseMetadataTrackColumns {
        .resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            ReleaseMetadataHeader(
                values: session.form,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                cover: { cover },
                context: { EmptyView() },
                sourceAudio: { sourceAudio }
            )
            trackTable
        }
        .disabled(session.isBusy)
    }

    private var cover: some View {
        Button {
            onEditCover?()
        } label: {
            ImageView(
                imageRef: session.cover,
                pointSize: ReleaseMetadataHeader<
                    EmptyView, EmptyView, EmptyView
                >
                .coverSize
            )
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(alignment: .topTrailing) {
                Image(systemName: "pencil")
                    .font(.caption2)
                    .foregroundStyle(.white)
                    .padding(3)
                    .background(.black.opacity(0.5))
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .padding(4)
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
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: String(localized: "Tracks"), ruled: true)
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    if session.trackSides.isEmpty {
                        headerRow
                        Text("No tracks")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .frame(width: tableWidth)
                            .padding(.vertical, 16)
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

    /// Shared audio gets a caption above its tracks. A source used by one
    /// track stays in that row, beside the track's editable metadata.
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
        Text(verbatim: text)
            .font(.system(size: 10, weight: .bold))
            .tracking(1.2)
            .textCase(.uppercase)
            .foregroundStyle(.secondary)
            .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
            .frame(width: tableWidth, alignment: .leading)
            .padding(.top, index == 0 ? 2 : 18)
            .padding(.bottom, 6)
    }

    private var headerRow: some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            FormEyebrow(text: Text("Source"))
                .frame(width: columns.source, alignment: .leading)
            FormEyebrow(text: Text("Track"))
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
            Color.clear.frame(width: ReleaseMetadataTrackColumns.action)
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.top, 4)
        .padding(.bottom, 6)
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
            Color.clear.frame(width: ReleaseMetadataTrackColumns.action)
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.vertical, 6)
        .frame(minHeight: 40)
        .overlay(alignment: .top) {
            Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
        }
    }

    private func sourceCell(
        _ item: ReleaseMetadataTrackItem,
        sourceIsInCaption: Bool
    ) -> some View {
        HStack(spacing: 8) {
            Button {
                onPlayTrack?(item.index)
            } label: {
                Image(systemName: "play.fill")
                    .font(.system(size: 10))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .opacity(onPlayTrack == nil ? 0 : 1)
            .allowsHitTesting(onPlayTrack != nil)
            if !sourceIsInCaption {
                let name = item.context.sources.map(\.name)
                    .joined(separator: " + ")
                Text(verbatim: name)
                    .font(.system(size: 12, design: .monospaced))
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
        HStack(spacing: 8) {
            Image(systemName: "list.bullet.rectangle")
                .foregroundStyle(.tertiary)
            Text(source.name)
                .font(.system(size: 12, weight: .medium, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.middle)
                .help(source.name)
        }
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.top, 8)
        .padding(.bottom, 6)
        .frame(width: tableWidth, alignment: .leading)
    }

    private func eyebrow(_ key: String) -> some View {
        FormEyebrow(text: Text(verbatim: coreString(key)))
    }
}

struct ReleaseSourceAudioSummaryView: View {
    let sourceAudio: BridgeSourceAudioSummary

    var body: some View {
        Text(sourceAudio.text)
            .font(.system(size: 11.5))
            .foregroundStyle(.tertiary)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityLabel(coreString("core.audio.label"))
            .accessibilityValue(sourceAudio.text)
    }
}
