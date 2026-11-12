import BaeKit
import SwiftUI

/// The context lane as the panel draws it: its length, the rows read so far,
/// and how to read more. A value the container builds from the store, so the
/// panel draws without one.
struct QueueUpcoming {
    let total: Int
    /// `nil` for a row not yet read.
    let itemAt: (Int) -> BridgeQueueEntry?
    let holds: (String) -> Bool
    let version: UInt64
    let loadRange: @MainActor (_ offset: Int, _ limit: Int) async -> Void
    /// Why the read failed, when it did.
    let failure: DisplayError?
    let onRetry: () -> Void
}

extension QueueUpcoming {
    /// The lane as the store holds it.
    @MainActor
    init(store: PlaybackStore, queue: Queue) {
        self.init(
            total: store.upcomingTotal,
            itemAt: { store.upcomingItem(at: $0) },
            holds: { store.upcomingHolds(entryId: $0) },
            version: store.contextVersion,
            loadRange: { offset, limit in
                await store.loadUpcomingRange(
                    offset: offset,
                    limit: limit,
                    queue: queue
                )
            },
            failure: store.upcomingFailure,
            onRetry: {
                Task { await store.retryUpcoming(queue: queue) }
            }
        )
    }
}

/// The queue docked as a fixed-width sidebar: a header, the now-playing card
/// when active, and the manual "Up next" and context lanes as `QueueSection`s
/// sharing one drag coordinator, drawn from the values it is given.
/// `MacQueuePanelContainer` reads the stores for it.
struct MacQueuePanel: View {
    /// The manual "Up next" lane, played first and always loaded in full.
    let manual: [BridgeQueueEntry]
    let manualVersion: UInt64
    /// What's playing from, or `nil`; its tracks come from `upcoming`.
    let context: QueuePlaybackContext?
    let upcoming: QueueUpcoming
    /// Where the playing track is; `nil` while none plays.
    let position: LatestSource<BridgeTrackPosition>?
    /// The context's length is read before any of its rows is shown.
    let onShowUpcoming: () -> Void
    var width: CGFloat = 420

    let isActive: Bool
    let nowPlayingTitle: String?
    let nowPlayingArtist: String?
    let nowPlayingCover: ImageContent?
    let isPlaying: Bool
    let onClose: () -> Void
    let onGoToNowPlaying: (() -> Void)?
    let onPlayPause: () -> Void
    let onClearUpNext: () -> Void
    let onClearPlayingFrom: () -> Void
    let onSkipTo: (String) -> Void
    let onRemove: (String) -> Void
    /// Move the entry `entryId` to sit before `beforeEntryId`; `nil` moves it
    /// to the end of its lane. Answers the lane's version the move made, or
    /// `nil` when the queue changed first and nothing moved.
    let onReorder:
        @MainActor (_ entryId: String, _ beforeEntryId: String?) async throws
            -> UInt64?
    /// Shows why a reorder failed.
    let onReorderFailed: @MainActor (any Error) -> Void
    let onInsertTracks: ([String], Int) -> Void
    /// Sets the playing context's shuffle, from the context section's header.
    let onSetShuffle: (Bool) -> Void

    // Shared because a context row can be dragged into the manual lane.
    @State
    private var dragCoordinator = QueueDragCoordinator()

    private var isEmpty: Bool {
        manual.isEmpty && (context == nil || upcoming.total == 0)
    }

    var body: some View {
        VStack(spacing: 0) {
            header

            if isActive {
                nowPlayingCard
            }

            if isEmpty {
                ContentUnavailableView {
                    Text("Queue is empty")
                } description: {
                    Text("Drag tracks here or play an album")
                }
                .frame(maxHeight: .infinity)
                .dropDestination(for: String.self) { dropped, _ in
                    guard !dropped.isEmpty else {
                        return false
                    }
                    onInsertTracks(dropped, 0)
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
                                ? nil : String(localized: "Up next"),
                            shuffled: false,
                            count: manual.count,
                            itemAt: { index in
                                manual.indices.contains(index)
                                    ? manual[index] : nil
                            },
                            holdsEntry: { id in
                                manual.contains { $0.id == id }
                            },
                            loadVersion: 0,
                            loadRange: nil,
                            acceptsExternalDrops: true,
                            laneId: .manual,
                            coordinator: dragCoordinator,
                            laneVersion: manualVersion,
                            onClear: onClearUpNext,
                            onSkipTo: onSkipTo,
                            onRemove: onRemove,
                            onReorder: onReorder,
                            onReorderFailed: onReorderFailed,
                            onInsertTracks: onInsertTracks,
                            onSetShuffle: nil,
                        )
                        .zIndex(dragCoordinator.isDragSource(.manual) ? 1 : 0)

                        if let context, upcoming.total > 0 {
                            // Partly read: `upcomingItem` returns `nil` for
                            // rows not yet read.
                            QueueSection(
                                title: Self.contextSectionTitle(context),
                                shuffled: context.shuffled,
                                count: upcoming.total,
                                itemAt: upcoming.itemAt,
                                holdsEntry: upcoming.holds,
                                loadVersion: upcoming.version,
                                loadRange: upcoming.loadRange,
                                acceptsExternalDrops: false,
                                laneId: .context,
                                coordinator: dragCoordinator,
                                laneVersion: upcoming.version,
                                onClear: onClearPlayingFrom,
                                onSkipTo: onSkipTo,
                                onRemove: onRemove,
                                onReorder: onReorder,
                                onReorderFailed: onReorderFailed,
                                onInsertTracks: onInsertTracks,
                                onSetShuffle: onSetShuffle,
                            )
                            .zIndex(
                                dragCoordinator.isDragSource(.context) ? 1 : 0
                            )
                            if let failure = upcoming.failure {
                                UpcomingFailureRow(
                                    failure: failure,
                                    onRetry: upcoming.onRetry
                                )
                            }
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
        .task(id: context != nil) {
            onShowUpcoming()
        }
        .frame(width: width)
        .frame(maxHeight: .infinity)
        // Sidebar chrome: one surface step above the content beside it, and
        // a hairline on the docked edge.
        .background(Theme.surface)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Theme.hairline)
                .frame(width: 1)
        }
    }

    /// The context section's title: "Playing from", plus the album title when
    /// known, for a release; "These tracks" for a list of tracks; "Your
    /// Library" for the library.
    private static func contextSectionTitle(_ context: QueuePlaybackContext)
        -> String
    {
        switch context.kind {
        case .release:
            guard let sourceTitle = context.sourceTitle else {
                return String(localized: "Playing from")
            }
            return String(localized: "Playing from · \(sourceTitle)")
        case .tracks:
            return String(localized: "These tracks")
        case .library:
            return String(localized: "Your library")
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(alignment: .top, spacing: ThemeSpace.group) {
            Text("Queue")
                .themeText(.title)
            Spacer(minLength: 0)
            MacCloseButton(onClose: onClose)
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.top, ThemeSpace.edge)
        .padding(.bottom, ThemeSpace.group)
    }

    // MARK: - Now Playing

    /// The now-playing card: cover, title, artist, and a progress strip on an
    /// elevated surface.
    private var nowPlayingCard: some View {
        HStack(alignment: .top, spacing: ThemeSpace.group) {
            nowPlayingArt
                .frame(
                    width: ThemeSize.barArtwork,
                    height: ThemeSize.barArtwork
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                .shadow(color: Theme.shadow, radius: 8, y: 4)
                .allowsHitTesting(false)

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                // Not `Eyebrow`: this label is accent, not secondary.
                Text("Now playing")
                    .themeText(.eyebrow)
                    .foregroundStyle(Theme.accent)
                if let title = nowPlayingTitle {
                    Text(title)
                        .themeText(.rowTitle)
                        .lineLimit(1)
                }
                if let artist = nowPlayingArtist {
                    Text(artist)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                ProgressStripRepresentable(position: position)
                    .frame(height: 10)
                    .padding(.top, ThemeSpace.compact)
            }
            .allowsHitTesting(false)

            Spacer(minLength: 0)

            PlayPauseControl(
                isPlaying: isPlaying,
                glyphFont: ThemeIcon.medium.font,
                targetSize: ThemeSize.hitTarget,
                onToggle: onPlayPause
            )
            .foregroundStyle(.secondary)
            .background(
                Theme.hover,
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
        }
        .padding(ThemeSpace.group)
        .background {
            Button {
                onGoToNowPlaying?()
            } label: {
                Color.clear
                    .contentShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.card)
                    )
            }
            .buttonStyle(.accentPlain)
            .disabled(onGoToNowPlaying == nil)
            .help("Go to now playing")
            .accessibilityLabel("Go to now playing")
        }
        .card(elevated: true)
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .strokeBorder(Theme.hairline, lineWidth: ThemeSpace.hairline)
        }
        .padding(.horizontal, ThemeSpace.group)
        .padding(.bottom, ThemeSpace.compact)
    }

    private var nowPlayingArt: some View {
        ImageView(content: nowPlayingCover, pointSize: ThemeSize.barArtwork)
    }
}

#if DEBUG
    // MARK: - Previews

#endif
