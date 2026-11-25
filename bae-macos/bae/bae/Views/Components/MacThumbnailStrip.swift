import BaeKit
import SwiftUI

/// A strip's sizes, outside the generic view so callers can read them.
enum MacThumbnailStripLayout {
    /// Each thumbnail's side, which callers also decode their images at.
    static let thumbnailSize: CGFloat = 56
    /// The strip's height: the room above and below each thumbnail is for
    /// its ring.
    static let height: CGFloat = 64
}

/// A horizontal strip of thumbnails that keeps the current item centered; the
/// caller supplies each item's image view and stroke.
struct MacThumbnailStrip<Item: Identifiable & Equatable, Content: View>: View {
    let items: [Item]
    /// The item shown now.
    let current: Item.ID
    /// Centers a short row and scrolls to the current item on appear.
    let centered: Bool
    let onSelect: (Item.ID) -> Void
    /// The item's name, which VoiceOver reads for its thumbnail button.
    let name: (Item) -> String
    /// Stroke (color, line width) for an item given whether it is the
    /// current item.
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
        .frame(height: MacThumbnailStripLayout.height)
    }

    private func strip(minWidth: CGFloat?) -> some View {
        ScrollViewReader { scrollProxy in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: ThemeSpace.compact) {
                    ForEach(items) { item in
                        cell(item)
                            .id(item.id)
                    }
                }
                .padding(.horizontal, ThemeSpace.related)
                .padding(
                    .vertical,
                    (MacThumbnailStripLayout.height
                        - MacThumbnailStripLayout.thumbnailSize) / 2
                )
                .frame(minWidth: minWidth)
            }
            .onAppear {
                if centered {
                    scrollProxy.scrollTo(current, anchor: .center)
                }
            }
            .onChange(of: current) { _, newId in
                withAnimation(.easeInOut(duration: 0.2)) {
                    scrollProxy.scrollTo(newId, anchor: .center)
                }
            }
        }
    }

    private func cell(_ item: Item) -> some View {
        let (color, lineWidth) = stroke(item, item.id == current)
        return Button {
            onSelect(item.id)
        } label: {
            content(item)
                .frame(
                    width: MacThumbnailStripLayout.thumbnailSize,
                    height: MacThumbnailStripLayout.thumbnailSize
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                // The whole ring lies outside the thumbnail, as the board's
                // ring does, so the strip holds room for it above and below.
                .overlay {
                    RoundedRectangle(
                        cornerRadius: ThemeRadius.artwork + lineWidth
                    )
                    .strokeBorder(color, lineWidth: lineWidth)
                    .padding(-lineWidth)
                }
        }
        .buttonStyle(.accentPlain)
        .accessibilityLabel(Text(verbatim: name(item)))
    }
}
