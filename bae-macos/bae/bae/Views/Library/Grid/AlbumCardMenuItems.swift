import SwiftUI

/// The SwiftUI rendering of an `AlbumCardMenu` — the card's `.contextMenu` items.
struct AlbumCardMenuItems: View {
    let menu: AlbumCardMenu

    var body: some View {
        Button(menu.playLabel) { menu.onPlay() }
        Button(menu.addToQueueLabel) { menu.onAddToQueue() }
        Button(menu.addNextLabel) { menu.onAddNext() }
    }
}

#if DEBUG
    #Preview("Album Card Menu Items") {
        Menu {
            AlbumCardMenuItems(menu: PreviewData.albumCardMenu())
        } label: {
            Text(verbatim: "Album Actions")
        }
        .menuStyle(.button)
        .padding()
        .frame(width: 240)
    }

    #Preview("Album Card Menu Items \u{2014} Multi") {
        Menu {
            AlbumCardMenuItems(menu: PreviewData.albumCardMenu(targetCount: 3))
        } label: {
            Text(verbatim: "Album Actions")
        }
        .menuStyle(.button)
        .padding()
        .frame(width: 240)
    }
#endif
