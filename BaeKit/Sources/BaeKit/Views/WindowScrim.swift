import SwiftUI

#if canImport(AppKit)
    import AppKit

    /// The corner radius of a Mac window, as the boards draw it.
    private let windowRadius: CGFloat = 17

    /// A window over all of another window, title bar included, that dims it
    /// with `Theme.scrim` and takes every click aimed at it, for a
    /// presentation that is a window of its own (`MacAlert`, a `MacSheet`'s
    /// system sheet). It is a child of the window it covers and follows its
    /// frame. The window server passes clicks through the clear parts of a
    /// window, so the scrim's colour is what takes them.
    @MainActor
    public final class ScrimWindow: NSWindow {
        public static let accessibilityIdentifier = "window-scrim"

        private let parent: NSWindow
        private var observers: [NSObjectProtocol] = []

        override public var canBecomeKey: Bool { false }
        override public var canBecomeMain: Bool { false }

        public init(over parent: NSWindow) {
            self.parent = parent
            super
                .init(
                    contentRect: parent.frame,
                    styleMask: [.borderless],
                    backing: .buffered,
                    defer: false
                )
            isReleasedWhenClosed = false
            isOpaque = false
            backgroundColor = .clear
            hasShadow = false
            animationBehavior = .none
            ignoresMouseEvents = false
            contentView = ScrimView()
            setAccessibilityElement(false)
            setAccessibilityIdentifier(Self.accessibilityIdentifier)
            for name in [
                NSWindow.didResizeNotification, NSWindow.didMoveNotification,
            ] {
                observers.append(
                    NotificationCenter.default.addObserver(
                        forName: name,
                        object: parent,
                        queue: .main
                    ) { [weak self] _ in
                        MainActor.assumeIsolated { self?.follow() }
                    }
                )
            }
        }

        private func follow() {
            setFrame(parent.frame, display: false)
        }

        override public func sendEvent(_ event: NSEvent) {}

        /// Puts the scrim over its parent.
        public func show() {
            follow()
            parent.addChildWindow(self, ordered: .above)
        }

        /// Takes the scrim off its parent.
        public func hide() {
            observers.forEach(NotificationCenter.default.removeObserver)
            observers = []
            parent.removeChildWindow(self)
            orderOut(nil)
        }
    }

    private final class ScrimView: NSView {
        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
            layer?.cornerRadius = windowRadius
            layer?.cornerCurve = .continuous
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
    }
#endif
