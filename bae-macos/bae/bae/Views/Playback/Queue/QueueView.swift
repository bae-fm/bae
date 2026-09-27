import BaeKit
import SwiftUI

/// The queue pane: a header, the now-playing card when active, and the manual
/// "Up Next" and context lanes as `QueueSection`s sharing one drag coordinator.
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
    let onGoToNowPlaying: (() -> Void)?
    let onPlayPause: () -> Void
    let onClearUpNext: () -> Void
    let onClearPlayingFrom: () -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void
    /// Move the entry `entryId` to sit before `beforeEntryId`; `nil` moves it
    /// to the end of its lane.
    let onReorder: (_ entryId: String, _ beforeEntryId: String?) -> Void
    let onInsertTracks: ([String], Int) -> Void
    /// Sets the playing context's shuffle, from the context section's header.
    let onSetShuffle: (Bool) -> Void

    // Shared because a context row can be dragged into the manual lane.
    @State
    private var dragCoordinator = QueueDragCoordinator()

    /// The manual "Up Next" lane, played first and always loaded in full.
    private var manual: [QueueItem] { playbackStore.manualQueue }
    /// What's playing from, or `nil`; `upcomingTotal` may exceed what's loaded.
    private var context: QueuePlaybackContext? { playbackStore.queueContext }

    private var isEmpty: Bool {
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
                    // A multi-selection drag carries several ids per payload.
                    let ids = droppedIds.flatMap(AlbumDragPayload.decode)
                    guard !ids.isEmpty else {
                        return false
                    }
                    onInsertTracks(ids, 0)
                    return true
                }
            }
            else {
                // No scroller: it only flashed as the pane animated in.
                ScrollView {
                    // Not lazy: `zIndex` has no effect in a lazy stack, and the
                    // drag-source section must draw above the other.
                    VStack(spacing: 0) {
                        // The manual lane plays first, and only it accepts
                        // external track drops.
                        QueueSection(
                            // Labelled only when it has rows.
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
                            // Partly loaded: `upcomingItem` returns `nil` for
                            // rows not yet fetched.
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
                    // The context section's cross-lane math needs this count.
                    dragCoordinator.manualGapCount = manual.count
                }
            }
        }
        // No background: QueuePanel supplies the panel material.
    }

    /// The context section's title: "Playing From", plus the album title when
    /// known, for a release; "Your Library" for the library.
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

    /// The now-playing card: cover, title, artist, and a progress strip on an
    /// elevated surface.
    private var nowPlayingCard: some View {
        HStack(alignment: .top, spacing: 12) {
            nowPlayingArt
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                .shadow(color: Theme.shadow, radius: 8, y: 4)
                .allowsHitTesting(false)

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
            .allowsHitTesting(false)

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
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
        }
        .padding(12)
        .background {
            Button {
                onGoToNowPlaying?()
            } label: {
                RoundedRectangle(cornerRadius: ThemeRadius.card)
                    .fill(Theme.surfaceElevated)
                    .contentShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.card)
                    )
            }
            .buttonStyle(.plain)
            .disabled(onGoToNowPlaying == nil)
            .help("Go to Now Playing")
            .accessibilityLabel("Go to Now Playing")
        }
        .overlay(
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .stroke(Theme.hairline, lineWidth: 1)
                .allowsHitTesting(false)
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

    /// The Queue preview and screenshot scene: the real pane over the
    /// environment store, with no-op commands.
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
                onGoToNowPlaying: nil,
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
