import BaeKit
import SwiftUI

/// A playlist's entries in order, each with the menu of a track in a
/// playlist. The playlist's owner moves an entry by dragging its row: the row
/// stays in place and dims, a copy follows the pointer, and a line over the
/// gap marks where the entry goes when the owner lets go; letting go writes
/// the move.
struct PlaylistEntryList: View {
    let detail: BridgePlaylistDetail
    let play: (BridgePlaylistEntryRow) -> Void
    let playNext: (BridgePlaylistEntryRow) -> Void
    let addToQueue: (BridgePlaylistEntryRow) -> Void
    let remove: (BridgePlaylistEntryRow) -> Void
    /// Move the entry `positionId` to the place `to` among the entries, where
    /// core changes the order, returning when the write returns.
    let move: (_ positionId: String, _ to: Int) async -> Void

    @State
    private var drag: PlaylistEntryDrag?
    /// The order just written, drawn until its write returns. Then the list
    /// draws the playlist's read, which shows what is stored.
    @State
    private var held: [String]?
    @State
    private var rowHeight: CGFloat = 0

    private static let space = "playlistEntries"
    /// How faint the dragged entry's own row is, and how faint the copy
    /// under the pointer is, as the board draws them.
    private static let draggedRowOpacity = 0.45
    private static let copyOpacity = 0.75

    private var canChange: Bool { detail.summary.yours }

    var body: some View {
        let entries = shown
        LazyVStack(spacing: 0) {
            ForEach(Array(entries.enumerated()), id: \.element.positionId) {
                index,
                entry in
                PlaylistEntryRow(
                    entry: entry,
                    play: { play(entry) },
                    playNext: { playNext(entry) },
                    addToQueue: { addToQueue(entry) },
                    remove: canChange ? { remove(entry) } : nil
                )
                .opacity(
                    drag?.entry.positionId == entry.positionId
                        ? Self.draggedRowOpacity : 1
                )
                .background(
                    GeometryReader { proxy in
                        Color.clear.preference(
                            key: PlaylistEntryHeightKey.self,
                            value: proxy.size.height
                        )
                    }
                )
                .gesture(
                    dragGesture(entry, index: index, count: entries.count),
                    including: canChange ? .all : .subviews
                )
            }
        }
        .coordinateSpace(name: Self.space)
        .onPreferenceChange(PlaylistEntryHeightKey.self) { rowHeight = $0 }
        .overlay(alignment: .topLeading) {
            if let drag, rowHeight > 0 {
                insertLine(drag: drag, count: entries.count)
                floatingCopy(drag)
            }
        }
    }

    /// The entries as drawn: the order just written until its write returns,
    /// else the query's rows.
    private var shown: [BridgePlaylistEntryRow] {
        guard let held else {
            return detail.entries
        }
        let byId = Dictionary(
            uniqueKeysWithValues: detail.entries.map { ($0.positionId, $0) }
        )
        return held.compactMap { byId[$0] }
    }

    /// The drag of `entry`, drawn at `index` among `count` rows.
    private func dragGesture(
        _ entry: BridgePlaylistEntryRow,
        index: Int,
        count: Int
    ) -> some Gesture {
        DragGesture(
            minimumDistance: ThemeSpace.inline,
            coordinateSpace: .named(Self.space)
        )
        .onChanged { value in
            guard rowHeight > 0 else { return }
            if drag == nil {
                drag = PlaylistEntryDrag(
                    entry: entry,
                    index: index,
                    grab: value.startLocation.y - CGFloat(index) * rowHeight,
                    pointerY: value.location.y
                )
            }
            drag?.pointerY = value.location.y
        }
        .onEnded { _ in
            guard let finished = drag else { return }
            drag = nil
            let target = finished.target(rowHeight: rowHeight, count: count)
            guard target != finished.index else { return }
            var order = shown.map(\.positionId)
            order.insert(order.remove(at: finished.index), at: target)
            held = order
            Task {
                await move(finished.entry.positionId, target)
                held = nil
            }
        }
    }

    /// The line over the gap the dragged entry moves to when the owner lets
    /// go; nothing while it would stay where it is.
    @ViewBuilder
    private func insertLine(drag: PlaylistEntryDrag, count: Int) -> some View {
        let target = drag.target(rowHeight: rowHeight, count: count)
        if target != drag.index {
            let gap = target > drag.index ? target + 1 : target
            Rectangle()
                .fill(Color.accentColor)
                .frame(height: 2)
                .padding(.horizontal, ThemeSpace.related)
                .offset(y: CGFloat(gap) * rowHeight - 1)
                .allowsHitTesting(false)
        }
    }

    /// The dragged entry's row drawn at the pointer.
    private func floatingCopy(_ drag: PlaylistEntryDrag) -> some View {
        PlaylistEntryRow(
            entry: drag.entry,
            play: {},
            playNext: {},
            addToQueue: {},
            remove: nil
        )
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(Theme.surfaceElevated)
        )
        .opacity(Self.copyOpacity)
        .shadow(
            color: Theme.shadow,
            radius: ThemeSpace.group,
            y: ThemeSpace.compact
        )
        .offset(y: drag.pointerY - drag.grab)
        .allowsHitTesting(false)
    }
}

/// The entry being dragged, the place its row was drawn at, where in the row
/// the pointer took hold, and where the pointer is, in the list's own space.
private struct PlaylistEntryDrag {
    let entry: BridgePlaylistEntryRow
    let index: Int
    let grab: CGFloat
    var pointerY: CGFloat

    /// The slot the entry would take: the one its dragged copy's middle is
    /// over, kept inside the list.
    func target(rowHeight: CGFloat, count: Int) -> Int {
        let middle = pointerY - grab + rowHeight / 2
        return min(max(Int((middle / rowHeight).rounded(.down)), 0), count - 1)
    }
}

private struct PlaylistEntryHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

/// One entry of a playlist: double-clicking plays the playlist from it, and
/// its menu plays it, queues it, and takes it out of the playlist when
/// `remove` is given.
private struct PlaylistEntryRow: View {
    let entry: BridgePlaylistEntryRow
    let play: () -> Void
    let playNext: () -> Void
    let addToQueue: () -> Void
    let remove: (() -> Void)?

    @State
    private var hovering = false

    var body: some View {
        MacListRow(
            lead: String(entry.number),
            cover: entry.cover,
            title: entry.title,
            subtitle: FactList.joined([entry.artistLine.text, entry.albumTitle]
            ),
            duration: DurationClock.label(entry.durationClock),
            hover: hovering
        )
        .onHover { hovering = $0 }
        .onTapGesture(count: 2, perform: play)
        .macContextMenu {
            MacMenuItem("Play") { play() }
            MacMenuItem("Play next") { playNext() }
            MacMenuItem("Add to queue") { addToQueue() }
            if let remove {
                MacMenuDivider()
                MacMenuItem("Remove from playlist") { remove() }
            }
        }
    }
}
