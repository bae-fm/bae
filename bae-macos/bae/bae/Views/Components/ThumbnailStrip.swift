import BaeKit
import SwiftUI

/// A strip's sizes, outside the generic view so callers can read them.
enum ThumbnailStripLayout {
    /// Each thumbnail's side, which callers also decode their images at.
    static let thumbnailSize: CGFloat = 56
}

/// A horizontal strip of thumbnails over a `Cursor` that keeps the current
/// item centered; the caller supplies each item's image view and stroke.
struct ThumbnailStrip<Item: Identifiable & Equatable, Content: View>: View {
    let cursor: Cursor<Item>
    /// Centers a short row and scrolls to the current item on appear.
    let centered: Bool
    let onSelect: (Item.ID) -> Void
    /// Stroke (color, line width) for an item given whether it is the
    /// cursor's current item.
    let stroke: (Item, Bool) -> (Color, CGFloat)
    @ViewBuilder
    let content: (Item) -> Content

    var body: some View {
        Group {
            if centered {
                GeometryReader { geo in
                    strip(minWidth: geo.size.width)
                }
            }
            else {
                strip(minWidth: nil)
            }
        }
        .frame(height: 64)
    }

    private func strip(minWidth: CGFloat?) -> some View {
        ScrollViewReader { scrollProxy in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: ThemeSpace.compact) {
                    ForEach(cursor.items) { item in
                        cell(item)
                            .id(item.id)
                    }
                }
                .padding(.horizontal, ThemeSpace.related)
                .frame(minWidth: minWidth)
            }
            .onAppear {
                if centered {
                    scrollProxy.scrollTo(cursor.current.id, anchor: .center)
                }
            }
            .onChange(of: cursor.current.id) { _, newId in
                withAnimation(.easeInOut(duration: 0.2)) {
                    scrollProxy.scrollTo(newId, anchor: .center)
                }
            }
        }
    }

    private func cell(_ item: Item) -> some View {
        let (color, lineWidth) = stroke(item, cursor.isCurrent(item))
        return Button {
            onSelect(item.id)
        } label: {
            content(item)
                .frame(
                    width: ThumbnailStripLayout.thumbnailSize,
                    height: ThumbnailStripLayout.thumbnailSize
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                .overlay(
                    RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        .stroke(color, lineWidth: lineWidth)
                )
        }
        .buttonStyle(.accentPlain)
    }
}
