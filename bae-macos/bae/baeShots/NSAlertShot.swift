import AppKit
import SwiftUI

/// An alert as AppKit lays it out: its window's content drawn into an image,
/// since a modal alert can't be captured offscreen.
struct NSAlertShot: View {
    let alert: () -> NSAlert

    @Environment(\.colorScheme)
    private var colorScheme

    var body: some View {
        let alert = alert()
        let window = alert.window
        window.appearance = NSAppearance(
            named: colorScheme == .dark ? .darkAqua : .aqua
        )
        alert.layout()
        guard let view = window.contentView,
            let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else {
            preconditionFailure("An alert's window has content to draw")
        }
        let bounds = view.bounds
        view.cacheDisplay(in: bounds, to: rep)
        let image = NSImage(size: bounds.size)
        image.addRepresentation(rep)
        return Image(nsImage: image)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
