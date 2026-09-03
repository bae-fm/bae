import SwiftUI

extension ImportMappingTable {
    var artistTrackIds: [String] {
        table.trackMappings.compactMap(\.track?.id)
    }

    private var artistFillFrame: CGRect? {
        guard let selection = artistFillSelection else { return nil }
        return selection.trackIds(in: artistTrackIds)
            .compactMap { artistCellFrames[$0] }
            .reduce(nil) { frame, cell in
                frame.map { $0.union(cell) } ?? cell
            }
    }

    /// The spreadsheet fill: a handle at the corner of the hovered row's
    /// artist cell; dragging it selects downward, an accent frame shows the
    /// range while the drag is live, and release applies the source row's
    /// artists to every selected row. Nothing persists past the drag — the
    /// handle exists under the pointer and the frame only while dragging, so
    /// no selection chrome is left standing after the fill.
    @ViewBuilder
    var artistFillOverlay: some View {
        if let anchor = fillHandleAnchor {
            ZStack(alignment: .topLeading) {
                if let frame = artistFillFrame {
                    Rectangle()
                        .stroke(Color.accentColor, lineWidth: 2)
                        .frame(width: frame.width, height: frame.height)
                        .position(x: frame.midX, y: frame.midY)
                        .allowsHitTesting(false)
                }
                ZStack {
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(Color.accentColor)
                        .frame(width: 8, height: 8)
                }
                .frame(width: 22, height: 22)
                .contentShape(Rectangle())
                .position(x: anchor.x, y: anchor.y)
                .highPriorityGesture(
                    DragGesture(
                        minimumDistance: 0,
                        coordinateSpace: .named(artistFillCoordinateSpace)
                    )
                    .onChanged { value in
                        if artistFillSelection == nil,
                            let hovered = hoveredFillTrackId
                        {
                            artistFillSelection = ArtistFillSelection(
                                sourceTrackId: hovered
                            )
                        }
                        extendArtistFill(to: value.location.y)
                    }
                    .onEnded { value in
                        extendArtistFill(to: value.location.y)
                        commitArtistFill()
                        artistFillSelection = nil
                    }
                )
            }
        }
    }

    /// Where the fill handle sits: the live selection's corner while a drag
    /// runs, else the hovered artist cell's.
    private var fillHandleAnchor: CGPoint? {
        if let frame = artistFillFrame {
            return CGPoint(x: frame.maxX, y: frame.maxY)
        }
        guard let hovered = hoveredFillTrackId,
            let cell = artistCellFrames[hovered]
        else { return nil }
        return CGPoint(x: cell.maxX, y: cell.maxY)
    }

    private func extendArtistFill(to y: CGFloat) {
        guard var selection = artistFillSelection,
            let sourceIndex = artistTrackIds.firstIndex(
                of: selection.sourceTrackId
            )
        else { return }
        let candidates = artistTrackIds[sourceIndex...]
            .compactMap { id in
                artistCellFrames[id].map { (id, $0) }
            }
        guard
            let target = candidates.min(by: {
                abs($0.1.midY - y) < abs($1.1.midY - y)
            })?
            .0
        else { return }
        selection.extend(to: target, in: artistTrackIds)
        artistFillSelection = selection
    }

    private func commitArtistFill() {
        guard let selection = artistFillSelection else { return }
        let trackIds = selection.trackIds(in: artistTrackIds)
        guard trackIds.count > 1,
            let assignments = table.trackMappings.compactMap(\.track)
                .first(where: { $0.id == selection.sourceTrackId })?
                .artistAssignments
        else { return }
        actions.setTrackArtists(trackIds, assignments)
    }
}

/// The artist cell that starts a spreadsheet fill and the last cell currently
/// covered by it. The table order defines the selected range.
struct ArtistFillSelection: Equatable {
    let sourceTrackId: String
    private(set) var throughTrackId: String

    init(sourceTrackId: String) {
        self.sourceTrackId = sourceTrackId
        throughTrackId = sourceTrackId
    }

    mutating func extend(to trackId: String, in orderedTrackIds: [String]) {
        guard let source = orderedTrackIds.firstIndex(of: sourceTrackId),
            let target = orderedTrackIds.firstIndex(of: trackId)
        else { return }
        throughTrackId = orderedTrackIds[max(source, target)]
    }

    func trackIds(in orderedTrackIds: [String]) -> [String] {
        guard let source = orderedTrackIds.firstIndex(of: sourceTrackId),
            let target = orderedTrackIds.firstIndex(of: throughTrackId),
            target >= source
        else { return [] }
        return Array(orderedTrackIds[source...target])
    }
}

struct ArtistCellFramePreferenceKey: PreferenceKey {
    static let defaultValue: [String: CGRect] = [:]

    static func reduce(
        value: inout [String: CGRect],
        nextValue: () -> [String: CGRect]
    ) {
        value.merge(nextValue(), uniquingKeysWith: { _, next in next })
    }
}
