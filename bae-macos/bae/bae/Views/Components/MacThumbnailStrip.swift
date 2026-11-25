import BaeKit
import SwiftUI

/// A strip's sizes, outside the generic view so callers can read them.
enum MacThumbnailStripLayout {
    /// Each thumbnail's side, which callers also decode their images at.
    static let thumbnailSize: CGFloat = 56
    /// The strip's height, which the thumbnails sit at the top of.
    static let height: CGFloat = 64
    /// The room above and below the strip that the scroll view holds, so it
    /// does not clip the outer half of a ring.
    static let ringRoom: CGFloat = 4
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
        // The scroll view reaches past the strip by the ring's room, which
        // the negative padding takes back out of the layout.
        .frame(
            height: MacThumbnailStripLayout.height
                + 2 * MacThumbnailStripLayout.ringRoom
        )
        .padding(.vertical, -MacThumbnailStripLayout.ringRoom)
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
                .padding(.vertical, MacThumbnailStripLayout.ringRoom)
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
        // The ring is beside the button, not on it, so VoiceOver's box for the
        // button is the thumbnail's own: the stroke lies outside the
        // thumbnail, as the board's ring does, and takes none of the layout.
        return ZStack {
            Button {
                onSelect(item.id)
            } label: {
                content(item)
                    .frame(
                        width: MacThumbnailStripLayout.thumbnailSize,
                        height: MacThumbnailStripLayout.thumbnailSize
                    )
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                    )
            }
            .buttonStyle(.accentPlain)
            .accessibilityLabel(Text(verbatim: name(item)))
            RoundedRectangle(
                cornerRadius: ThemeRadius.artwork,
                style: .circular
            )
            .inset(by: -lineWidth / 2)
            .stroke(color, lineWidth: lineWidth)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
        .frame(
            width: MacThumbnailStripLayout.thumbnailSize,
            height: MacThumbnailStripLayout.thumbnailSize
        )
    }
}
