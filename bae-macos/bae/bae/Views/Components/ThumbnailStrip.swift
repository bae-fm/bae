import SwiftUI

/// Horizontal strip of 56pt thumbnails over a `Cursor`, used by the lightbox
/// and the import cover picker. Owns the scroll scaffolding (animated
/// scroll-to-center when the current item changes) and the shared cell
/// chrome (56×56 frame, 6pt rounded clip, stroke overlay, click-to-select);
/// the caller supplies the per-item image view and the stroke for its
/// selection states.
struct ThumbnailStrip<Item: Identifiable & Equatable, Content: View>: View {
    let cursor: Cursor<Item>
    /// Lightbox presentation: stretch the row to at least the container
    /// width so a short row centers, and scroll the current item to center
    /// when the strip first appears. The cover picker leaves this off
    /// (leading-aligned, no initial scroll).
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
                HStack(spacing: 6) {
                    ForEach(cursor.items) { item in
                        cell(item)
                            .id(item.id)
                    }
                }
                .padding(.horizontal, 8)
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
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(color, lineWidth: lineWidth)
                )
        }
        .buttonStyle(.plain)
    }
}
