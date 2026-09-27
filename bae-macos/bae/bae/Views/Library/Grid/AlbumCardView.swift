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

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            albumArt
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .shadow(color: Theme.shadow, radius: 14, y: 9)
                // The ring floats outside the cover's edge rather than eating
                // into it.
                .overlay(
                    RoundedRectangle(cornerRadius: 16)
                        .inset(by: -4.5)
                        .stroke(
                            isExpanded ? Theme.accent : .clear,
                            lineWidth: 3
                        )
                )
                .overlay(alignment: .topTrailing) {
                    CardMenuButton(menu: menu, showMenu: $showMenu)
                        .padding(6)
                        .opacity(isHovered || showMenu ? 1 : 0)
                        .allowsHitTesting(isHovered || showMenu)
                }
                .onHover { isHovered = $0 }
                .padding(.bottom, 10)
            Text(title)
                .font(.system(size: 15, weight: .bold))
                .lineLimit(1)
            Text(artistNames)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            StableOptionalText(
                text: year.map(String.init),
                font: .system(size: 12, weight: .medium),
                foreground: .tertiary,
                lineHeight: 14
            )
        }
        .padding(6)
        .background(
            RoundedRectangle(cornerRadius: 10)
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
        HStack(spacing: 20) {
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
