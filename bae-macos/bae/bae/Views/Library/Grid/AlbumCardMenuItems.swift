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
