import BaeKit
import SwiftUI

/// The queue pane: a header, the now-playing card (when active), and the two
/// lanes — the manual "Up Next" lane and the context (what's playing from) —
/// rendered as stacked `QueueSection`s over a shared `QueueDragCoordinator`.
struct QueueView: View {
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Queue.self)
    private var queue

    let isActive: Bool
    let nowPlayingTitle: String?
    let nowPlayingArtist: String?
    let nowPlayingCover: ImageContent?
    let isPlaying: Bool
    let isLoading: Bool
    let onClose: () -> Void
    let onPlayPause: () -> Void
    let onClearUpNext: () -> Void
    let onClearPlayingFrom: () -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void
    /// Move the entry `entryId` to sit before `beforeEntryId`; `nil` moves it
    /// to the end of its lane.
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void
    /// Flip the playing context between sequential and shuffled order. Wired only
    /// to the context section's header — shuffle is a property of the context.
    let onSetShuffle: (Bool) -> Void

    // A drag can start in either section and target the other (a context row
    // enqueues into the manual lane), so the coordinator is shared; hover and
    // external-drop insertion stay per-section (positional — a row index in
    // one lane must not light up the same index in the other).
    @State
    private var dragCoordinator = QueueDragCoordinator()

    /// The manual lane ("Up Next"): explicitly enqueued tracks, drained first —
    /// always resolved in full, never windowed.
    private var manual: [QueueItem] { playbackStore.manualQueue }
    /// The context (the release being played from), or `nil` when nothing plays
    /// from a release. Rendered as a section distinct from the manual lane;
    /// `upcomingTotal` may exceed what's currently loaded.
    private var context: QueuePlaybackContext? { playbackStore.queueContext }

    private var isEmpty: Bool {
        // No context (nothing playing from a release) contributes no rows — a
        // normal state, named here rather than folded into a default.
        switch context {
        case .none:
            return manual.isEmpty
        case .some(let context):
            return manual.isEmpty && context.upcomingTotal == 0
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header

            if isActive {
                nowPlayingCard
            }

            if isEmpty {
                ContentUnavailableView(
                    "Queue is empty",
                    systemImage: "list.bullet",
                    description: Text("Drag tracks here or play an album"),
                )
                .frame(maxHeight: .infinity)
                .dropDestination(for: String.self) { droppedIds, _ in
                    // A dragged album card may carry several ids joined by a
                    // newline (a multi-selection drag); split each back out.
                    let ids = droppedIds.flatMap(AlbumDragPayload.decode)
                    guard !ids.isEmpty else {
                        return false
                    }
                    onInsertTracks(ids, 0)
                    return true
                }
            }
            else {
                // No overlay scroller: inside the fixed-size popover it only
                // ever showed up as a flash when the pane animates in.
                ScrollView {
                    // A plain VStack, NOT LazyVStack: zIndex is inert across a
                    // lazy container's children (each cell composites in its
                    // own layer, in order), so the drag-source section could
                    // never rise above the other — a manual-lane row dragged
                    // over the context section rendered underneath it. The
                    // laziness deferred nothing anyway: the sections are the
                    // only two children, and each materializes wholesale as
                    // one cell.
                    VStack(spacing: 0) {
                        // The manual lane drains first, so it is shown first. It
                        // accepts external track drops (Play Next / Add to Queue
                        // land here); the context section, being the release's own
                        // order, takes reorder/remove/skip only. It is always
                        // fully resolved, so it never has a load hook or unloaded
                        // rows.
                        QueueSection(
                            // The manual lane labels itself only when it has
                            // rows; the context header below always shows (it
                            // names what's playing, not just the lane).
                            title: manual.isEmpty
                                ? nil : String(localized: "Up Next"),
                            shuffled: false,
                            count: manual.count,
                            itemAt: { index in
                                manual.indices.contains(index)
                                    ? manual[index] : nil
                            },
                            loadEpoch: 0,
                            loadRange: nil,
                            acceptsExternalDrops: true,
                            laneId: .manual,
                            coordinator: dragCoordinator,
                            queueRevision: playbackStore.revision,
                            onClear: onClearUpNext,
                            onSkipTo: onSkipTo,
                            onRemove: onRemove,
                            onReorder: onReorder,
                            onInsertTracks: onInsertTracks,
                            onSetShuffle: nil,
                        )
                        .zIndex(dragCoordinator.isDragSource(.manual) ? 1 : 0)

                        if let context, context.upcomingTotal > 0 {
                            // The context tail is library-scaled and only
                            // partly resolved (`upcomingItem` returns `nil` for
                            // an index not yet loaded); the section renders a
                            // placeholder for those rows and fetches the range
                            // around them.
                            QueueSection(
                                title: Self.contextSectionTitle(context),
                                shuffled: context.shuffled,
                                count: context.upcomingTotal,
                                itemAt: { playbackStore.upcomingItem(at: $0) },
                                loadEpoch: playbackStore.revision,
                                loadRange: { offset, limit in
                                    await playbackStore.loadUpcomingRange(
                                        offset: offset,
                                        limit: limit,
                                        queue: queue
                                    )
                                },
                                acceptsExternalDrops: false,
                                laneId: .context,
                                coordinator: dragCoordinator,
                                queueRevision: playbackStore.revision,
                                onClear: onClearPlayingFrom,
                                onSkipTo: onSkipTo,
                                onRemove: onRemove,
                                onReorder: onReorder,
                                onInsertTracks: onInsertTracks,
                                onSetShuffle: onSetShuffle,
                            )
                            .zIndex(
                                dragCoordinator.isDragSource(.context) ? 1 : 0
                            )
                        }
                    }
                }
                .coordinateSpace(name: "queuePane")
                .scrollIndicators(.never)
                .onChange(of: manual.count, initial: true) {
                    // Cross-lane gap math runs in the context section's
                    // gesture, which can't see the manual section's props.
                    dragCoordinator.manualGapCount = manual.count
                }
            }
        }
        // No background of its own: QueuePanel supplies the panel material —
        // an opaque fill here would block it.
    }

    /// The context section's title, by what it plays from: a release keeps the
    /// "Playing From" label — suffixed with the album title when core resolved
    /// one — and the library names itself. Resolving a localized key by the
    /// source kind is the UI's locale-rendering job — the kind and title cross
    /// the bridge as data, the prose stays here.
    private static func contextSectionTitle(_ context: QueuePlaybackContext)
        -> String
    {
        switch context.kind {
        case .release:
            guard let sourceTitle = context.sourceTitle else {
                return String(localized: "Playing From")
            }
            return String(localized: "Playing From") + " · " + sourceTitle
        case .library:
            return String(localized: "Your Library")
        }
    }

    // MARK: - Header

    // Each lane clears itself from its own section header; this one names the
    // pane and closes it.
    private var header: some View {
        HStack(alignment: .top, spacing: 12) {
            Text("Queue")
                .font(.system(size: 22, weight: .heavy))
            Spacer(minLength: 0)
            PanelCloseButton(onClose: onClose)
        }
        .padding(.horizontal, 20)
        .padding(.top, 18)
        .padding(.bottom, 12)
    }

    // MARK: - Now Playing

    /// The now-playing card: cover, title/artist, and a slim progress strip
    /// on an elevated neutral surface above the queue lanes.
    private var nowPlayingCard: some View {
        HStack(alignment: .top, spacing: 12) {
            nowPlayingArt
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: 9))
                .shadow(color: .black.opacity(0.35), radius: 8, y: 4)

            VStack(alignment: .leading, spacing: 2) {
                Text("Now Playing")
                    .font(.system(size: 9, weight: .bold))
                    .tracking(1.3)
                    .textCase(.uppercase)
                    .foregroundStyle(Theme.accent)
                if let title = nowPlayingTitle {
                    Text(title)
                        .font(.system(size: 14, weight: .bold))
                        .lineLimit(1)
                }
                if let artist = nowPlayingArtist {
                    Text(artist)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                ProgressStripRepresentable()
                    .frame(height: 10)
                    .padding(.top, 6)
            }

            Spacer(minLength: 0)

            PlayPauseControl(
                isPlaying: isPlaying,
                isLoading: isLoading,
                glyphFont: .system(size: 12, weight: .semibold),
                spinnerControlSize: .small,
                targetSize: 30,
                onToggle: onPlayPause
            )
            .foregroundStyle(.secondary)
            .background(
                Theme.hover,
                in: RoundedRectangle(cornerRadius: 9)
            )
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 12).fill(Theme.surfaceElevated)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Theme.hairline, lineWidth: 1)
        )
        .padding(.horizontal, 14)
        .padding(.bottom, 6)
    }

    private var nowPlayingArt: some View {
        ImageView(content: nowPlayingCover, pointSize: 56)
    }
}

#if DEBUG
    // MARK: - Previews

    /// The Queue preview and screenshot composition. It renders the production
    /// pane with queue state from the environment store and no-op commands, so
    /// preview interactions settle back instead of changing the fixture.
    @MainActor
    struct QueueViewPreviewScene: View {
        enum Presentation {
            case populated
            case empty
        }

        let width: CGFloat
        @Environment(PlaybackStore.self)
        private var store

        static func store(for presentation: Presentation) -> PlaybackStore {
            switch presentation {
            case .populated:
                let store = PreviewData.queueStore(
                    manualCount: 2,
                    shuffled: true
                )
                store.play(
                    track: NowPlayingTrack(
                        trackId: "preview-now-playing",
                        trackTitle: PreviewData.nowPlayingTitle,
                        artistNames: PreviewData.nowPlayingArtist,
                        albumId: "preview-album",
                        coverImage: nil,
                        durationMs: 214_000
                    )
                )
                return store
            case .empty:
                return PreviewData.queueStore(
                    manualCount: 0,
                    context: nil
                )
            }
        }

        var body: some View {
            QueueView(
                isActive: store.nowPlaying.isActive,
                nowPlayingTitle: store.nowPlaying.track?.trackTitle,
                nowPlayingArtist: store.nowPlaying.track?.artistNames,
                nowPlayingCover: nil,
                isPlaying: store.nowPlaying.isPlaying,
                isLoading: store.nowPlaying.loadingTrackId != nil,
                onClose: {},
                onPlayPause: {},
                onClearUpNext: {},
                onClearPlayingFrom: {},
                onSkipTo: { _ in },
                onRemove: { _ in },
                onReorder: { _, _ in },
                onInsertTracks: { _, _ in },
                onSetShuffle: { _ in }
            )
            .frame(width: width, height: 720)
            .background(Theme.surface)
        }
    }

    #Preview("With items") {
        QueueViewPreviewScene(width: 420)
            .environment(
                QueueViewPreviewScene.store(for: .populated)
            )
            .environment(Queue.stub())
            .environment(ImageStore.stub())
    }

    #Preview("Empty") {
        QueueViewPreviewScene(width: 420)
            .environment(QueueViewPreviewScene.store(for: .empty))
            .environment(Queue.stub())
            .environment(ImageStore.stub())
    }
#endif
