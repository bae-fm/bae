#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// An alert's accessory: the alert's usual 260 less its margins.
    @MainActor
    public enum AlertAccessory {
        public static let width: CGFloat = 228

        /// `content` at the accessory's width and its fitting height.
        public static func view(_ content: some View) -> NSView {
            let hosting = NSHostingView(
                rootView:
                    content
                    .frame(width: width)
                    .fixedSize(horizontal: false, vertical: true)
            )
            let height = hosting.fittingSize.height
            hosting.frame = NSRect(x: 0, y: 0, width: width, height: height)
            return hosting
        }
    }
#endif
