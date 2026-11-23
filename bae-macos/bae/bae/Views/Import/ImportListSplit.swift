import AppKit
import BaeKit
import SwiftUI

/// The import list beside its pane, with a divider the person drags to
/// resize the list.
///
/// The list opens at its ideal width, 420 points, as the board draws it, and
/// the divider moves it between its floor, 410, and its ceiling, 460, never
/// leaving the pane less than its own floor.
struct ImportListSplit<List: View, Pane: View>: View {
    static var floorWidth: CGFloat { 410 }
    static var idealWidth: CGFloat { 420 }
    static var ceilingWidth: CGFloat { 460 }

    let paneFloor: CGFloat
    @ViewBuilder
    let list: () -> List
    @ViewBuilder
    let pane: () -> Pane

    @State
    private var listWidth = Self.idealWidth
    /// The list's width when the current drag began.
    @State
    private var widthAtDragStart: CGFloat?

    var body: some View {
        GeometryReader { proxy in
            let width = clamped(listWidth, in: proxy.size.width)
            HStack(spacing: 0) {
                list()
                    .frame(width: width)
                    .frame(maxHeight: .infinity)
                    .overlay(alignment: .trailing) {
                        divider(width: width, in: proxy.size.width)
                    }
                pane()
            }
        }
    }

    private func clamped(_ width: CGFloat, in total: CGFloat) -> CGFloat {
        let ceiling = max(
            Self.floorWidth,
            min(Self.ceilingWidth, total - paneFloor)
        )
        return min(max(width, Self.floorWidth), ceiling)
    }

    /// A hairline on the list's trailing edge, with a wider strip that takes
    /// the drag and shows the resize cursor.
    private func divider(width: CGFloat, in total: CGFloat) -> some View {
        MacHairline(axis: .vertical)
            .overlay {
                Color.clear
                    .frame(width: ThemeSpace.inline * 2)
                    .contentShape(Rectangle())
                    .onHover { inside in
                        if inside {
                            NSCursor.resizeLeftRight.push()
                        }
                        else {
                            NSCursor.pop()
                        }
                    }
                    .gesture(
                        DragGesture(coordinateSpace: .global)
                            .onChanged { drag in
                                let start = widthAtDragStart ?? width
                                widthAtDragStart = start
                                listWidth = clamped(
                                    start + drag.translation.width,
                                    in: total
                                )
                            }
                            .onEnded { _ in widthAtDragStart = nil }
                    )
            }
    }
}
