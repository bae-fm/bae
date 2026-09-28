import BaeKit
import SwiftUI

/// The expanded album-detail card; `AlbumDetailView` owns the state and
/// supplies every callback.
struct AlbumExpansionContent: View {
    let summary: AlbumSummary
    /// Fat detail for the release the user is currently viewing.
    let selectedRelease: ReleaseDetail
    let onBrowseImages: () -> Void
    /// Cursor over the album's releases; drives the release picker.
    @Binding
    var releaseCursor: Cursor<ReleaseRef>
    let currentTrackId: String?
    /// The track currently loading, whose row shows a spinner.
    let loadingTrackId: String?
    let isPlaying: Bool
    let onClose: () -> Void
    let onPlay: () -> Void
    let onShuffle: () -> Void
    let onPlayFromTrack: (Int) -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onAddNextAlbum: () -> Void
    let onAddAlbumToQueue: () -> Void
    let onChangeCover: () -> Void
    let onEditMetadata: () -> Void
    let onReIdentify: () -> Void
    let onOpenStorage: () -> Void
    let onExportRelease: () -> Void
    let onSaveReleaseAs: () -> Void
    let onSetPrimaryRelease: () -> Void
    let onDeleteRelease: () -> Void
    let onExportTrack: (String) -> Void

    /// The album cover's side.
    private static let coverSize: CGFloat = 340

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: ThemeSpace.page) {
                albumArt
                    .frame(width: Self.coverSize, height: Self.coverSize)
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.cover)
                    )
                    .shadow(color: Theme.shadow, radius: 20, y: 12)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onBrowseImages)
                VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                    Text(summary.title)
                        .themeText(.hero)
                        .lineLimit(1)
                    HStack(spacing: ThemeSpace.related) {
                        Text(summary.artistNames)
                            .foregroundStyle(.secondary)
                        if let year = summary.year {
                            Text(verbatim: "\u{00B7}")
                                .foregroundStyle(.tertiary)
                            Text(String(year))
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .themeText(.heading)
                    .lineLimit(1)
                    if releaseCursor.canCycle {
                        releasePicker
                    }
                    ReleaseFactsLine(
                        pressingLine: selectedRelease.pressingLine,
                        labelsLine: selectedRelease.labelsLine,
                        records: selectedRelease.records
                    )
                    HStack(spacing: ThemeSpace.related) {
                        Button(action: onPlay) {
                            Label("Play", systemImage: "play.fill")
                        }
                        .buttonStyle(PrimaryButtonStyle())
                        albumMenu
                    }
                    .padding(.top, ThemeSpace.group)
                    AlbumTrackListView(
                        release: selectedRelease,
                        isCompilation: summary.isCompilation,
                        currentTrackId: currentTrackId,
                        loadingTrackId: loadingTrackId,
                        isPlaying: isPlaying,
                        onPlayFromTrack: onPlayFromTrack,
                        onTogglePlayPause: onTogglePlayPause,
                        onAddNext: onAddNext,
                        onAddToQueue: onAddToQueue,
                        onExportTrack: onExportTrack,
                    )
                    .padding(.top, ThemeSpace.edge)
                }
            }
        }
        .padding(ThemeSpace.page)
        .background(
            Theme.surfaceElevated,
            in: RoundedRectangle(cornerRadius: ThemeRadius.panel)
        )
        .overlay(
            RoundedRectangle(cornerRadius: ThemeRadius.panel)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        )
        .shadow(color: Theme.shadow, radius: 28, y: 18)
        .overlay(alignment: .topTrailing) {
            PanelCloseButton(onClose: onClose)
                .padding(ThemeSpace.edge)
        }
    }

    private var canSetAsPrimaryRelease: Bool {
        selectedRelease.id != summary.primaryReleaseId
    }

    private var albumMenu: some View {
        Menu {
            Button(action: onPlay) {
                Label("Play", systemImage: "play.fill")
            }
            Button(action: onShuffle) {
                Label("Shuffle", systemImage: "shuffle")
            }
            Divider()
            Button(action: { onAddNextAlbum() }) {
                Label(
                    "Play Next",
                    systemImage: "text.line.first.and.arrowtriangle.forward"
                )
            }
            Button(action: { onAddAlbumToQueue() }) {
                Label("Add to Queue", systemImage: "text.append")
            }
            Divider()
            Button("Change Cover...") { onChangeCover() }
            Button("Edit metadata...") { onEditMetadata() }
            Button("Re-identify...") { onReIdentify() }
            Button("Storage...") { onOpenStorage() }
            Button("Export…") { onExportRelease() }
            Button("Save As…") { onSaveReleaseAs() }
            if releaseCursor.canCycle, canSetAsPrimaryRelease {
                Divider()
                Button("Set as Primary Release") { onSetPrimaryRelease() }
            }
            Divider()
            Button(role: .destructive, action: onDeleteRelease) {
                Label("Delete Release", systemImage: "trash")
            }
        } label: {
            Image(systemName: "ellipsis")
                .themeIcon(.medium)
                .foregroundStyle(.secondary)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .background(
                    Theme.hover,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    private var albumArt: some View {
        ImageView(
            imageRef: selectedRelease.summary.cover,
            pointSize: Self.coverSize
        )
    }

    private var releasePicker: some View {
        NativeSegmentedControl(
            selectedIndex: Binding(
                get: { releaseCursor.index },
                set: { newIndex in
                    // AppKit's segmented control can report an index out of
                    // range.
                    guard releaseCursor.items.indices.contains(newIndex) else {
                        return
                    }
                    releaseCursor.select(id: releaseCursor.items[newIndex].id)
                },
            ),
            segments: releaseCursor.items.enumerated()
                .map { index, ref in
                    NativeSegmentedControl.Segment(
                        label: libraryStore.releaseDetails[ref.id]?.displayName
                            ?? String(localized: "Release \(index + 1)"),
                        systemImage: releaseContainsCurrentTrack(id: ref.id)
                            ? "speaker.fill" : nil,
                    )
                },
        )
        .padding(.top, ThemeSpace.inline)
        .padding(.bottom, ThemeSpace.line)
    }

    private func releaseContainsCurrentTrack(id: String) -> Bool {
        guard let currentTrackId else {
            return false
        }
        guard let detail = libraryStore.releaseDetails[id] else {
            return false
        }
        return detail.tracks.contains(where: { $0.id == currentTrackId })
    }
}

/// The release's facts on two lines, each cut short on its own: the pressing,
/// then its labels. When a catalog describes the release, an arrow beside the
/// first line marks them and a click toggles a card naming the catalogs.
///
/// The card is drawn in the window rather than as a popover, so it carries its
/// own monitor that closes it on a click away or Escape.
struct ReleaseFactsLine: View {
    let pressingLine: String
    let labelsLine: String
    let records: [BridgeReleaseRecord]

    /// How far under the line the card's top sits.
    private static let cardOffset = ThemeSpace.related

    @State
    private var isHovering = false
    @State
    private var isShowingCard = false
    @State
    private var trigger = OverlayAnchor()
    /// The facts' own height, which is where the card hangs from.
    @State
    private var lineHeight: CGFloat = 0

    var body: some View {
        if records.isEmpty {
            factsText
        }
        else {
            Button {
                isShowingCard.toggle()
            } label: {
                HStack(
                    alignment: .firstTextBaseline,
                    spacing: ThemeSpace.inline
                ) {
                    factsText
                    Image(systemName: "arrow.up.right")
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .allowsHitTesting(false)
                        .accessibilityLabel(
                            coreString("core.identity.identified")
                        )
                }
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.chip)
                        .fill(isHovering ? Theme.hover : Color.clear)
                )
                // The fill bleeds outward without moving the text.
                .padding(.horizontal, -ThemeSpace.compact)
                .padding(.vertical, -ThemeSpace.line)
            }
            .buttonStyle(.plain)
            .background { OverlayTrigger(anchor: trigger) }
            .onHover { isHovering = $0 }
            .accessibilityIdentifier("release-facts")
            .onGeometryChange(for: CGFloat.self) { geometry in
                geometry.size.height
            } action: {
                lineHeight = $0
            }
            .overlay(alignment: .topLeading) {
                if isShowingCard {
                    card.offset(y: lineHeight + Self.cardOffset)
                }
            }
            // Over the rows that follow the line, which the card lies across.
            .zIndex(1)
        }
    }

    private var card: some View {
        ReleaseRecordsCard(records: records)
            .background(
                Theme.tile,
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
            .overlay {
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .strokeBorder(Theme.hairline, lineWidth: 1)
            }
            .shadow(color: Theme.shadow, radius: 14, y: 8)
            .background {
                OverlayDismissMonitor(trigger: trigger) {
                    isShowingCard = false
                }
            }
            .fixedSize()
            .accessibilityIdentifier("release-facts-card")
    }

    /// The lines the release states, each on one line of its own.
    var shownLines: [String] {
        [pressingLine, labelsLine].filter { !$0.isEmpty }
    }

    private var factsText: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            ForEach(Array(shownLines.enumerated()), id: \.offset) { _, line in
                Text(line)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
        .themeText(.detail)
        .foregroundStyle(.tertiary)
    }
}

#if DEBUG
    #Preview("Single Disc") {
        PreviewData.albumExpansionScene(
            albumId: "a-01",
            currentTrackId: "t-d1-2",
            isPlaying: true
        )
    }

    #Preview("Single Disc — Track Loading") {
        PreviewData.albumExpansionScene(
            albumId: "a-01",
            currentTrackId: "t-d1-2",
            loadingTrackId: "t-d1-3",
            isPlaying: true
        )
    }

    #Preview("Vinyl — Two Sides") {
        // The album-detail gallery scene renders this exact composition.
        PreviewScenes.albumDetail()
    }

    #Preview("CD — Two Discs") {
        PreviewData.albumExpansionScene(albumId: "a-22")
    }

    #Preview("CD — A Split Disc Over a Whole One") {
        // The album-detail-mixed-columns gallery scene renders this exact
        // composition.
        PreviewScenes.albumDetailMixedColumns()
    }

    private struct MultiReleasePreview: View {
        @State
        private var selectedReleaseId: String = "rel-a-04-0"
        // Seeded at construction: the body is empty until the store holds the
        // album, and an empty body never fires `onAppear`.
        @State
        private var store = PreviewData.seededLibraryStore()

        var body: some View {
            let summary = store.albumSummaries["a-04"]
            let selected = store.releaseDetails[selectedReleaseId]
            if let summary, let selected {
                PreviewData.albumExpansionContent(
                    summary: summary,
                    selectedRelease: selected,
                    // Live cursor so selecting a release in the picker cycles
                    // the preview to that release's detail.
                    releaseCursor: Binding(
                        get: {
                            PreviewData.releaseCursor(
                                releaseIds: summary.releaseIds,
                                preferring: selectedReleaseId
                            )
                        },
                        set: { selectedReleaseId = $0.current.id },
                    ),
                    currentTrackId: "t-d2-3",
                    isPlaying: true,
                )
                .padding()
                .frame(width: 1100)
                .background(Theme.background)
                .environment(UiStore())
                .environment(store)
                .environment(ImageStore.stub())
            }
            else {
                // A preview with nothing to render is a broken fixture, not an
                // empty state.
                Text(
                    verbatim:
                        "release \(selectedReleaseId) of a-04 is not in PreviewData"
                )
            }
        }
    }

    #Preview("Multiple Releases") {
        MultiReleasePreview()
            .environment(ImageStore.stub())
            .environment(UiStore())
            .environment(LibraryStore())
    }
#endif
