#if os(macOS)
    import AppKit

    /// The focus ring of `accentFocusRing`, for a control AppKit draws: a layer
    /// round the view, three points wide in the accent at 45 percent, so it
    /// takes no room. The control turns its own ring off (`focusRingType`
    /// `.none`) and calls `update(in:)` when its focus, its colour, its
    /// size or its appearance changes.
    @MainActor
    public final class AccentFocusRingLayer {
        public var color = NSColor.clear
        public var shown = false
        private let cornerRadius: CGFloat
        private let layer = CAShapeLayer()

        private static let width: CGFloat = 3
        private static let opacity: CGFloat = 0.45

        public init(cornerRadius: CGFloat) {
            self.cornerRadius = cornerRadius
            layer.fillColor = nil
            layer.lineWidth = Self.width
        }

        public func update(in view: NSView) {
            view.wantsLayer = true
            if layer.superlayer == nil {
                view.layer?.addSublayer(layer)
            }
            layer.isHidden = !shown
            view.effectiveAppearance.performAsCurrentDrawingAppearance {
                layer.strokeColor =
                    color.withAlphaComponent(Self.opacity).cgColor
            }
            let radius = cornerRadius + Self.width / 2
            layer.path = CGPath(
                roundedRect: view.bounds.insetBy(
                    dx: -Self.width / 2,
                    dy: -Self.width / 2
                ),
                cornerWidth: radius,
                cornerHeight: radius,
                transform: nil
            )
        }
    }
#endif
