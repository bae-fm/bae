import BaeKit
import SwiftUI

/// One album card in the grid. Hover and selection chrome toggle by opacity so
/// a state change never re-measures the row.
struct AlbumCardView: View {
    let title: String
    let artistNames: String
    let year: Int32?
    let cover: BridgeImageRef?
    /// The album's detail is open; shown as the accent ring on the art.
    let isExpanded: Bool
    /// The album is in the multi-selection; shown as a tint behind the card.
    let isSelected: Bool
    let size: CGFloat
    let menu: AlbumCardMenu

    @State
    private var isHovered = false
    @State
    private var showMenu = false

    /// How far the expanded ring sits outside the cover's edge.
    private static let ringOutset: CGFloat = 4.5

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            albumArt
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
                .shadow(color: Theme.shadow, radius: 14, y: 9)
                .overlay(
                    RoundedRectangle(
                        cornerRadius: ThemeRadius.cover + Self.ringOutset
                    )
                    .inset(by: -Self.ringOutset)
                    .stroke(
                        isExpanded ? Theme.accent : .clear,
                        lineWidth: 3
                    )
                )
                .overlay(alignment: .topTrailing) {
                    CardMenuButton(menu: menu, showMenu: $showMenu)
                        .padding(ThemeSpace.compact)
                        .opacity(isHovered || showMenu ? 1 : 0)
                        .allowsHitTesting(isHovered || showMenu)
                }
                .onHover { isHovered = $0 }
                .padding(.bottom, ThemeSpace.related)
            Text(title)
                .themeText(.rowTitle)
                .lineLimit(1)
            Text(artistNames)
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            StableOptionalText(
                text: year.map(String.init),
                font: ThemeText.detail.font,
                foreground: .tertiary,
                lineHeight: 14
            )
        }
        .padding(ThemeSpace.compact)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .fill(Theme.accentSoft)
                .opacity(isSelected ? 1 : 0)
        )
        .contextMenu {
            AlbumCardMenuItems(menu: menu)
        }
    }

    private var albumArt: some View {
        ImageView(imageRef: cover, pointSize: size)
            .frame(width: size, height: size)
    }
}

#if DEBUG
    #Preview("Album Card") {
        let album = PreviewData.albums[0]
        let selected = PreviewData.albums[3]
        let menu = AlbumCardMenu(
            targetCount: 1,
            onPlay: {},
            onAddToQueue: {},
            onAddNext: {}
        )
        HStack(spacing: ThemeSpace.section) {
            AlbumCardView(
                title: album.title,
                artistNames: album.artistNames,
                year: album.year,
                cover: nil,
                isExpanded: false,
                isSelected: false,
                size: 200,
                menu: menu,
            )
            AlbumCardView(
                title: selected.title,
                artistNames: selected.artistNames,
                year: selected.year,
                cover: nil,
                isExpanded: false,
                isSelected: true,
                size: 200,
                menu: menu,
            )
        }
        .padding()
        .environment(ImageStore.stub())
    }
#endif
