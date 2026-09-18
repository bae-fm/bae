import BaeKit
import SwiftUI

/// The expanded album-detail card: cover, title block, release picker, primary
/// actions, and the track list. Pure prop-driven content — its wiring parent
/// `AlbumDetailView` owns the state and supplies every callback.
struct AlbumExpansionContent: View {
    let summary: AlbumSummary
    /// Fat detail for the release the user is currently viewing.
    let selectedRelease: ReleaseDetail
    let onBrowseImages: () -> Void
    /// Cursor over the album's releases. Drives the release picker and
    /// guarantees a valid selection on every read.
    @Binding
    var releaseCursor: Cursor<ReleaseRef>
    let currentTrackId: String?
    /// The id of the track currently loading (cloud download / decode warm-up),
    /// or nil. The matching row shows a spinner where its play/speaker glyph goes.
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

    @Environment(LibraryStore.self)
    private var libraryStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 36) {
                albumArt
                    .frame(width: 340, height: 340)
                    .clipShape(RoundedRectangle(cornerRadius: 14))
                    .shadow(color: .black.opacity(0.6), radius: 20, y: 12)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onBrowseImages)
                VStack(alignment: .leading, spacing: 4) {
                    Text(summary.title)
                        .font(.system(size: 30, weight: .heavy))
                        .tracking(-0.5)
                        .lineLimit(1)
                    HStack(spacing: 8) {
                        Text(summary.artistNames)
                            .foregroundStyle(.secondary)
                        if let year = summary.year {
                            Text(verbatim: "\u{00B7}")
                                .foregroundStyle(.tertiary)
                            Text(String(year))
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .font(.system(size: 15, weight: .semibold))
                    .lineLimit(1)
                    if releaseCursor.canCycle {
                        releasePicker
                    }
                    ReleaseFactsLine(
                        facts: selectedRelease.compactMetadata,
                        marks: selectedRelease.marks,
                        verification: selectedRelease.verification,
                        records: selectedRelease.records
                    )
                    HStack(spacing: 10) {
                        Button(action: onPlay) {
                            Label("Play", systemImage: "play.fill")
                        }
                        .buttonStyle(PrimaryButtonStyle())
                        albumMenu
                    }
                    .padding(
                        EdgeInsets(top: 12, leading: 0, bottom: 0, trailing: 0)
                    )
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
                    .padding(.top, 18)
                }
            }
        }
        .padding(32)
        .background(
            Theme.surfaceElevated,
            in: RoundedRectangle(cornerRadius: 18)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 18)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.45), radius: 28, y: 18)
        .overlay(alignment: .topTrailing) {
            PanelCloseButton(onClose: onClose)
                .padding(16)
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
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 36, height: 36)
                .background(
                    Theme.hover,
                    in: RoundedRectangle(cornerRadius: 10)
                )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    private var albumArt: some View {
        ImageView(imageRef: selectedRelease.summary.cover, pointSize: 340)
    }

    private var releasePicker: some View {
        NativeSegmentedControl(
            selectedIndex: Binding(
                get: { releaseCursor.index },
                set: { newIndex in
                    // Guard against an out-of-range index from AppKit's segmented
                    // control. `Cursor.select(id:)` is a no-op for unknown ids.
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
        .padding(EdgeInsets(top: 4, leading: 0, bottom: 2, trailing: 0))
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

/// The release's facts, and the way into where they came from.
///
/// At rest the line reads as it always has. When the release carries names of
/// its own, the rip databases confirmed its audio, or a catalog describes it,
/// hovering fills the line softly and fades a seal in at its tail, and a
/// click toggles a card under it stating all three. A release with none of
/// them has nothing behind the line, so it is not a trigger at all.
///
/// The card is drawn in the window, anchored to the line's leading edge and
/// laid over whatever sits under it, rather than as a popover floating off
/// the line with an arrow: it is part of the expansion, not a window of its
/// own. That means nothing closes it for us, so the card carries the monitor
/// that closes it on a click away or Escape.
private struct ReleaseFactsLine: View {
    let facts: String
    let marks: [BridgeReleaseMark]
    let verification: BridgeVerification?
    let records: [BridgeReleaseRecord]

    /// How far under the line the card's top sits.
    private static let cardOffset: CGFloat = 8

    @State
    private var isHovering = false
    @State
    private var isShowingCard = false
    @State
    private var trigger = OverlayAnchor()
    /// The line's own height, which is where the card hangs from.
    @State
    private var lineHeight: CGFloat = 0

    var body: some View {
        if marks.isEmpty, records.isEmpty, verification?.matchedCopies == nil {
            factsText
        }
        else {
            Button {
                isShowingCard.toggle()
            } label: {
                HStack(spacing: 6) {
                    factsText
                    Image(systemName: "seal")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .opacity(isHovering ? 1 : 0)
                        .animation(
                            .easeInOut(duration: 0.15),
                            value: isHovering
                        )
                        .accessibilityLabel(
                            coreString("core.identity.identified")
                        )
                }
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(
                    RoundedRectangle(cornerRadius: 5)
                        .fill(isHovering ? Theme.hover : Color.clear)
                )
                // The fill bleeds outward from where the line already sat, so
                // the card reads the same at rest as it did before it became
                // a trigger.
                .padding(.horizontal, -5)
                .padding(.vertical, -2)
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
                    // Hung off the line's bottom edge: the card's top sits
                    // the offset below it.
                    card.offset(y: lineHeight + Self.cardOffset)
                }
            }
            // Over the rows that follow the line, which the card lies across.
            .zIndex(1)
        }
    }

    private var card: some View {
        ReleaseFactsPopover(
            marks: marks,
            verification: verification,
            records: records
        )
        .background(Theme.tile, in: RoundedRectangle(cornerRadius: 9))
        .overlay {
            RoundedRectangle(cornerRadius: 9)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.5), radius: 14, y: 8)
        .background {
            OverlayDismissMonitor(trigger: trigger) {
                isShowingCard = false
            }
        }
        .fixedSize()
        .accessibilityIdentifier("release-facts-card")
    }

    private var factsText: some View {
        Text(facts)
            .font(.system(size: 12, weight: .medium))
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
        // Seeded at construction: the body has nothing to show until the
        // store holds the album, and an empty body never appears, so seeding
        // from `onAppear` never ran.
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
