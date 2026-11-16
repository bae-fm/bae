import BaeKit
import SwiftUI

/// A playlist's entries in order, each with the menu of a track in a
/// playlist. The playlist's owner moves an entry by dragging its row: the row
/// stays in place and dims, a copy follows the pointer, and a line marks the
/// edge of the row the entry would go beside; letting go writes the move.
struct PlaylistEntryList: View {
    let detail: BridgePlaylistDetail
    let play: (BridgePlaylistEntryRow) -> Void
    let playNext: (BridgePlaylistEntryRow) -> Void
    let addToQueue: (BridgePlaylistEntryRow) -> Void
    let remove: (BridgePlaylistEntryRow) -> Void
    /// Move the entry `positionId` to `place`, where core finds the entry's
    /// new place in the playlist, returning when the write returns.
    let move: (_ positionId: String, _ place: BridgeEntryPlace) async -> Void

    @State
    private var drag: PlaylistEntryDrag?
    /// The order the list wrote, which it draws until the write returns. Then
    /// the list draws the playlist's rows, in the order the library stores.
    @State
    private var held: [String]?
    /// Where the list draws each row it has on screen, by position id.
    @State
    private var rowFrames: [String: CGRect] = [:]

    private static let space = "playlistEntries"
    /// How faint the dragged entry's own row is, and how faint the copy
    /// under the pointer is, as the board draws them.
    private static let draggedRowOpacity = 0.45
    private static let copyOpacity = 0.75

    private var canChange: Bool { detail.summary.yours }

    var body: some View {
        LazyVStack(spacing: 0) {
            ForEach(shown, id: \.positionId) { entry in
                // While a move's write runs, the numbers core gave the rows
                // no longer match the order drawn, so the rows draw none.
                PlaylistEntryRow(
                    entry: entry,
                    number: held == nil ? entry.number : nil,
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
                            key: PlaylistEntryFramesKey.self,
                            value: [
                                entry.positionId: proxy.frame(
                                    in: .named(Self.space)
                                )
                            ]
                        )
                    }
                )
                .gesture(
                    dragGesture(entry),
                    including: canChange ? .all : .subviews
                )
            }
        }
        .coordinateSpace(name: Self.space)
        .onPreferenceChange(PlaylistEntryFramesKey.self) { rowFrames = $0 }
        .overlay(alignment: .topLeading) {
            if let drag {
                insertLine(drag.target(over: rowFrames))
                floatingCopy(drag)
            }
        }
    }

    /// The entries as drawn: the order the list wrote until its write
    /// returns, else the playlist's rows.
    private var shown: [BridgePlaylistEntryRow] {
        guard let held else {
            return detail.entries
        }
        let byId = Dictionary(
            uniqueKeysWithValues: detail.entries.map { ($0.positionId, $0) }
        )
        return held.compactMap { byId[$0] }
    }

    /// The person's drag of `entry`.
    private func dragGesture(_ entry: BridgePlaylistEntryRow) -> some Gesture {
        DragGesture(
            minimumDistance: ThemeSpace.inline,
            coordinateSpace: .named(Self.space)
        )
        .onChanged { value in
            if drag == nil {
                guard let frame = rowFrames[entry.positionId] else { return }
                drag = PlaylistEntryDrag(
                    entry: entry,
                    grab: value.startLocation.y - frame.minY,
                    height: frame.height,
                    pointerY: value.location.y
                )
            }
            drag?.pointerY = value.location.y
        }
        .onEnded { _ in
            guard let finished = drag else { return }
            drag = nil
            guard let target = finished.target(over: rowFrames) else { return }
            held = target.order(moving: finished.entry, in: shown)
            Task {
                await move(finished.entry.positionId, target.place)
                held = nil
            }
        }
    }

    /// The line at the edge of the row the dragged entry would go beside;
    /// nothing while the copy is over no other row.
    @ViewBuilder
    private func insertLine(_ target: PlaylistEntryTarget?) -> some View {
        if let target {
            MacQueueInsertLine()
                .offset(y: target.edge - 1)
        }
    }

    /// The dragged entry's row drawn at the pointer.
    private func floatingCopy(_ drag: PlaylistEntryDrag) -> some View {
        PlaylistEntryRow(
            entry: drag.entry,
            number: drag.entry.number,
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

/// The entry the person is dragging, where in its row the pointer took hold,
/// the row's height, and where the pointer is, in the list's own space.
private struct PlaylistEntryDrag {
    let entry: BridgePlaylistEntryRow
    let grab: CGFloat
    let height: CGFloat
    var pointerY: CGFloat

    /// The row the dragged copy's middle is over, from the rows' `frames`,
    /// and whether the entry goes before or after it; nil while the middle
    /// is over the dragged entry's own row or over no row.
    func target(over frames: [String: CGRect]) -> PlaylistEntryTarget? {
        let middle = pointerY - grab + height / 2
        guard
            let (positionId, frame) = frames.first(where: {
                $0.value.minY <= middle && middle < $0.value.maxY
            }),
            positionId != entry.positionId
        else {
            return nil
        }
        let after = middle >= frame.midY
        return PlaylistEntryTarget(
            beside: positionId,
            after: after,
            edge: after ? frame.maxY : frame.minY
        )
    }
}

/// Where a dropped entry goes: just before or just after the entry `beside`,
/// and the y of the row edge the line marks.
private struct PlaylistEntryTarget {
    let beside: String
    let after: Bool
    let edge: CGFloat

    /// The place core is sent.
    var place: BridgeEntryPlace {
        after ? .after(positionId: beside) : .before(positionId: beside)
    }

    /// The order of `rows`' ids with `moved` at this place, which the list
    /// draws until the move's write returns.
    func order(
        moving moved: BridgePlaylistEntryRow,
        in rows: [BridgePlaylistEntryRow]
    ) -> [String] {
        var order = rows.map(\.positionId).filter { $0 != moved.positionId }
        let at = order.firstIndex(of: beside).map { after ? $0 + 1 : $0 }
        order.insert(moved.positionId, at: at ?? order.count)
        return order
    }
}

/// Each drawn row's frame in the list's space, by position id.
private struct PlaylistEntryFramesKey: PreferenceKey {
    static let defaultValue: [String: CGRect] = [:]

    static func reduce(
        value: inout [String: CGRect],
        nextValue: () -> [String: CGRect]
    ) {
        value.merge(nextValue()) { $1 }
    }
}

/// One entry of a playlist: double-clicking plays the playlist from it, and
/// its menu plays it, queues it, and takes it out of the playlist when
/// `remove` is given.
private struct PlaylistEntryRow: View {
    let entry: BridgePlaylistEntryRow
    /// The entry's number in the playlist, or nil to draw none.
    let number: UInt64?
    let play: () -> Void
    let playNext: () -> Void
    let addToQueue: () -> Void
    let remove: (() -> Void)?

    @State
    private var hovering = false

    var body: some View {
        MacListRow(
            lead: number.map { String($0) } ?? "",
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
            PlaylistEntryMenu(
                play: play,
                playNext: playNext,
                addToQueue: addToQueue,
                remove: remove
            )
        }
    }
}
