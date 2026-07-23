import SwiftUI

/// The SwiftUI rendering of an `AlbumCardMenu` — the card's `.contextMenu` items.
struct AlbumCardMenuItems: View {
    let menu: AlbumCardMenu

    var body: some View {
        Button(menu.playLabel) { menu.onPlay() }
        Button(menu.addToQueueLabel) { menu.onAddToQueue() }
        Button(menu.addNextLabel) { menu.onAddNext() }
        Button(menu.pinLabel) { menu.onPin() }
    }
}

#if DEBUG
    #Preview("Album Card Menu Items") {
        Menu("Album Actions") {
            AlbumCardMenuItems(menu: PreviewData.albumCardMenu())
        }
        .menuStyle(.button)
        .padding()
        .frame(width: 240)
    }

    #Preview("Album Card Menu Items \u{2014} Multi") {
        Menu("Album Actions") {
            AlbumCardMenuItems(menu: PreviewData.albumCardMenu(targetCount: 3))
        }
        .menuStyle(.button)
        .padding()
        .frame(width: 240)
    }
#endif
