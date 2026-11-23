import BaeKit
import SwiftUI

/// One album card in the grid. Hover and selection chrome toggle by opacity so
/// a state change never re-measures the row.
struct MacAlbumTile: View {
    let title: String
    let artistNames: String?
    let year: Int32?
    let cover: BridgeImageRef?
    /// The album's detail is open; shown as the accent ring on the art.
    let isExpanded: Bool
    /// The album is in the multi-selection; shown as a tint behind the card.
    let isSelected: Bool
    let isUnplayable: Bool
    let isHovered: Bool
    let width: CGFloat
    let menu: AlbumCardMenu

    @Environment(\.accessibilityVoiceOverEnabled)
    private var voiceOverEnabled

    /// Whether the ⋯ menu is open, which keeps the ⋯ shown after the pointer
    /// leaves the cover for the menu.
    @State
    private var menuOpen = false

    private var showsMenu: Bool { isHovered || menuOpen || voiceOverEnabled }

    var body: some View {
        AlbumCardStack(
            title: title,
            artistNames: artistNames,
            year: year,
            isUnplayable: isUnplayable,
            width: width
        ) { side in
            Artwork(
                imageRef: cover,
                pointSize: side,
                ring: isExpanded,
                shadow: .tile,
                dimmed: isUnplayable
            )
            .overlay(alignment: .topTrailing) {
                MacMoreMenu(
                    look: .overlay,
                    label: Text("Album options"),
                    isOpen: $menuOpen
                ) {
                    MacMenuItem(
                        verbatim: menu.playLabel,
                        action: menu.onPlay
                    )
                    MacMenuItem.divider
                    MacMenuItem(
                        verbatim: menu.addToQueueLabel,
                        action: menu.onAddToQueue
                    )
                    MacMenuItem(
                        verbatim: menu.addNextLabel,
                        action: menu.onAddNext
                    )
                    if let onDelete = menu.onDelete {
                        MacMenuItem.divider
                        MacMenuItem(
                            verbatim: menu.deleteLabel,
                            action: onDelete
                        )
                    }
                }
                .padding(ThemeSpace.compact)
                .opacity(showsMenu ? 1 : 0)
                .allowsHitTesting(showsMenu)
                .accessibilityHidden(!showsMenu)
            }
        }
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.card, style: .circular)
                .fill(Theme.accentSoft)
                .opacity(isSelected ? 1 : 0)
        )
        .contextMenu {
            AlbumCardMenuItems(menu: menu)
        }
        .accessibilityElement(children: .contain)
    }
}

struct MacAlbumTileContainer: View {
    let album: BridgeAlbum
    let artistNames: String?
    let isExpanded: Bool
    let isSelected: Bool
    let isUnplayable: Bool
    let width: CGFloat
    let menu: AlbumCardMenu

    @State
    private var isHovered = false

    var body: some View {
        MacAlbumTile(
            title: album.title,
            artistNames: artistNames,
            year: album.year,
            cover: album.cover,
            isExpanded: isExpanded,
            isSelected: isSelected,
            isUnplayable: isUnplayable,
            isHovered: isHovered,
            width: width,
            menu: menu
        )
        .contentShape(Rectangle())
        .onHover { isHovered = $0 }
    }
}

/// The space the grid reserves while an album's row loads.
struct AlbumCardPlaceholder: View {
    let width: CGFloat

    var body: some View {
        AlbumCardStack(title: " ", artistNames: " ", year: nil, width: width) {
            side in
            Color.clear.frame(width: side, height: side)
        }
        .hidden()
    }
}

/// A card's layout: the art over the title, the artists and the year.
private struct AlbumCardStack<Art: View>: View {
    let title: String
    let artistNames: String?
    let year: Int32?
    var isUnplayable = false
    let width: CGFloat
    @ViewBuilder
    let art: (CGFloat) -> Art

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            art(max(0, width - 2 * ThemeSpace.compact))
                .padding(.bottom, ThemeSpace.compact)
            Text(title)
                .themeText(.rowTitle)
                .foregroundStyle(Theme.textPrimary)
                .lineLimit(1)
                .unplayableAlbumText(isUnplayable, names: true)
            if let artistNames {
                Text(artistNames)
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
                    .lineLimit(1)
                    .unplayableAlbumText(isUnplayable)
            }
            StableOptionalText(
                text: year.map(String.init),
                role: .detail,
                foreground: .tertiary
            )
            .unplayableAlbumText(isUnplayable)
        }
        .padding(ThemeSpace.compact)
        .frame(width: width, alignment: .leading)
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
            onAddNext: {},
            onDelete: {}
        )
        HStack(spacing: ThemeSpace.section) {
            MacAlbumTile(
                title: album.title,
                artistNames: album.artistLine.text,
                year: album.year,
                cover: nil,
                isExpanded: false,
                isSelected: false,
                isUnplayable: false,
                isHovered: false,
                width: 200,
                menu: menu,
            )
            MacAlbumTile(
                title: selected.title,
                artistNames: selected.artistLine.text,
                year: selected.year,
                cover: nil,
                isExpanded: false,
                isSelected: true,
                isUnplayable: false,
                isHovered: false,
                width: 200,
                menu: menu,
            )
        }
        .padding()
        .environment(ImageStore.stub())
    }
#endif
