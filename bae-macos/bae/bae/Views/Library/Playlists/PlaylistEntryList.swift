import BaeKit
import SwiftUI
import UniformTypeIdentifiers

/// A playlist's entries in order, each with the menu of a track in a
/// playlist. The playlist's owner moves an entry by dragging its row: the row
/// stays in place and dims, a copy follows the pointer, and a line marks the
/// edge of the row the entry would go beside; letting go writes the move.
/// Tracks or albums dragged from the window over the owner's rows draw the
/// same line, and letting go of them writes them in at that place.
struct PlaylistEntryList: View {
    let detail: BridgePlaylistDetail
    let play: (BridgePlaylistEntryRow) -> Void
    let playNext: (BridgePlaylistEntryRow) -> Void
    let addToQueue: (BridgePlaylistEntryRow) -> Void
    let remove: (BridgePlaylistEntryRow) -> Void
    /// Move the entry `positionId` to `place`, where core finds the entry's
    /// new place in the playlist, returning when the write returns.
    let move: (_ positionId: String, _ place: BridgeEntryPlace) async -> Void
    /// Put what `dropped` names at `place`, each string an album drag's
    /// payload or a track's id, returning when the write returns.
    let insert: (_ dropped: [String], _ place: BridgeBesideEntry) async -> Void

    @State
    private var drag: PlaylistEntryDrag?
    /// The order the list wrote, which it draws until the write returns. Then
    /// the list draws the playlist's rows, in the order the library stores.
    @State
    private var held: [String]?
    /// Where the list draws each row it has on screen, by position id.
    @State
    private var rowFrames: [String: CGRect] = [:]
    /// Where the tracks dragged in from the window would go, while the
    /// pointer holds them over a row.
    @State
    private var dropping: PlaylistEntryTarget?
    /// Where the list wrote dropped tracks, which it marks until the write
    /// returns and the playlist's rows hold them.
    @State
    private var inserting: PlaylistEntryTarget?

    private static let space = "playlistEntries"

    private var canChange: Bool { detail.summary.yours }

    var body: some View {
        LazyVStack(spacing: 0) {
            ForEach(shown, id: \.positionId) { entry in
                // While a move's write runs, the numbers core gave the rows
                // no longer match the order drawn, so the rows draw none.
                PlaylistEntryRow(
                    entry: entry,
                    number: held == nil ? entry.number : nil,
                    lift: drag?.entry.positionId == entry.positionId
                        ? .lifted : .resting,
                    play: { play(entry) },
                    playNext: { playNext(entry) },
                    addToQueue: { addToQueue(entry) },
                    remove: canChange ? { remove(entry) } : nil
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
        .onDrop(
            of: [UTType.plainText],
            delegate: PlaylistDropDelegate(
                accepts: canChange,
                frames: rowFrames,
                dropping: $dropping,
                drop: drop
            )
        )
        .overlay(alignment: .topLeading) {
            insertLine(drag?.target(over: rowFrames) ?? inserting ?? dropping)
            if let drag {
                floatingCopy(drag)
            }
        }
    }

    /// Write what `dropped` names in at `target`, marking the place until the
    /// write returns.
    private func drop(_ dropped: [String], at target: PlaylistEntryTarget) {
        inserting = target
        Task {
            await insert(dropped, target.place)
            inserting = nil
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
                await move(
                    finished.entry.positionId,
                    .beside(place: target.place)
                )
                held = nil
            }
        }
    }

    /// The line at the edge of the row the dragged entry or tracks would go
    /// beside; nothing while they are over no row they can go beside.
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
            lift: .dragging,
            play: {},
            playNext: {},
            addToQueue: {},
            remove: nil
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
        guard let target = PlaylistEntryTarget(at: middle, over: frames),
            target.beside != entry.positionId
        else {
            return nil
        }
        return target
    }
}

/// Where a dropped entry or track goes: just before or just after the entry
/// `beside`, and the y of the row edge the line marks.
private struct PlaylistEntryTarget {
    let beside: String
    let after: Bool
    let edge: CGFloat

    /// The place by the row at `y` among the rows' `frames`: after the row
    /// when `y` is in its lower half, else before it; nil over no row.
    init?(at y: CGFloat, over frames: [String: CGRect]) {
        guard
            let (positionId, frame) = frames.first(where: {
                $0.value.minY <= y && y < $0.value.maxY
            })
        else {
            return nil
        }
        beside = positionId
        after = y >= frame.midY
        edge = after ? frame.maxY : frame.minY
    }

    /// The place core is sent.
    var place: BridgeBesideEntry {
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

/// Tracks and albums dragged in from the window over the playlist's rows:
/// while they are held over a row, `dropping` is the place by it, and
/// letting go hands their strings and that place to `drop`. Only an owner's
/// list `accepts` them.
private struct PlaylistDropDelegate: DropDelegate {
    let accepts: Bool
    /// Each drawn row's frame, in the list's space, where the drop's
    /// location is too.
    let frames: [String: CGRect]
    @Binding
    var dropping: PlaylistEntryTarget?
    let drop: (_ dropped: [String], _ target: PlaylistEntryTarget) -> Void

    func validateDrop(info: DropInfo) -> Bool {
        accepts && info.hasItemsConforming(to: [UTType.plainText])
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        dropping = PlaylistEntryTarget(at: info.location.y, over: frames)
        return DropProposal(operation: dropping == nil ? .forbidden : .copy)
    }

    func dropExited(info _: DropInfo) {
        dropping = nil
    }

    func performDrop(info: DropInfo) -> Bool {
        let target = PlaylistEntryTarget(at: info.location.y, over: frames)
        dropping = nil
        guard let target else { return false }
        info.loadPlainText { dropped in
            drop(dropped, target)
        }
        return true
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
    /// Where the row is in the owner's drag of an entry.
    let lift: MacListRow.Lift
    let play: () -> Void
    let playNext: () -> Void
    let addToQueue: () -> Void
    let remove: (() -> Void)?

    @State
    private var hovering = false

    var body: some View {
        MacListRow(
            lead: number.map { String($0) } ?? "",
            leadWidth: MacPlaylistPage.numberWidth,
            hanging: true,
            coverSize: 40,
            cover: entry.cover,
            title: entry.title,
            subtitle: entry.artistLine.text,
            duration: DurationClock.label(entry.durationClock),
            hover: hovering,
            lift: lift
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
