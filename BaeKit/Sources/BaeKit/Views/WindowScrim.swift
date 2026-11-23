import SwiftUI

#if canImport(AppKit)
    import AppKit

    /// Dims all of `window`, its title bar included, with `Theme.scrim`, for
    /// a presentation that is a window of its own over it. The scrim is a
    /// view across the window's frame view, above the content and the title
    /// bar, so it takes the window's rounded shape.
    @MainActor
    public enum WindowScrim {
        public static let accessibilityIdentifier = "window-scrim"

        /// Dims `window`. Remove the returned view when the presentation
        /// ends.
        public static func show(over window: NSWindow) -> NSView? {
            guard let frameView = window.contentView?.superview else {
                return nil
            }
            let scrim = ScrimView(frame: frameView.bounds)
            scrim.autoresizingMask = [.width, .height]
            frameView.addSubview(scrim, positioned: .above, relativeTo: nil)
            return scrim
        }
    }

    private final class ScrimView: NSView {
        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
            setAccessibilityElement(false)
            setAccessibilityIdentifier(WindowScrim.accessibilityIdentifier)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError()
        }

        override var wantsUpdateLayer: Bool { true }

        override func updateLayer() {
            effectiveAppearance.performAsCurrentDrawingAppearance {
                layer?.backgroundColor = NSColor(Theme.scrim).cgColor
            }
        }

        /// The scrim takes every click aimed at the window below it.
        override func hitTest(_ point: NSPoint) -> NSView? {
            frame.contains(point) ? self : nil
        }
    }
#endif
