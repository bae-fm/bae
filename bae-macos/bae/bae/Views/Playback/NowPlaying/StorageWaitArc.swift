import AppKit
import BaeKit
import SwiftUI

/// An arc turning around the play/pause control while `waits` is true: the
/// now-playing value's `waitsOnStorage` flag.
struct StorageWaitArc: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme

    /// Whether the loading track's load waits on storage.
    let waits: Bool

    func makeNSView(context _: Context) -> StorageWaitArcNSView {
        let view = StorageWaitArcNSView()
        apply(to: view)
        return view
    }

    func updateNSView(_ view: StorageWaitArcNSView, context _: Context) {
        apply(to: view)
    }

    private func apply(to view: StorageWaitArcNSView) {
        view.color = NSColor(accent.color(in: colorScheme))
        view.show(waits)
    }
}

/// The arc itself: a quarter of a circle inside the view's bounds, turning
/// once a second while shown, hidden while the load waits on nothing.
final class StorageWaitArcNSView: NSView {
    private static let strokeWidth: CGFloat = 2.5
    private static let turnKey = "turn"

    private let arc = CAShapeLayer()

    var color: NSColor = .controlAccentColor {
        didSet { arc.strokeColor = color.cgColor }
    }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        arc.fillColor = nil
        arc.lineWidth = Self.strokeWidth
        arc.lineCap = .round
        arc.strokeStart = 0
        arc.strokeEnd = 0.25
        arc.strokeColor = color.cgColor
        layer?.addSublayer(arc)
        isHidden = true
        setAccessibilityElement(true)
        setAccessibilityRole(.progressIndicator)
        setAccessibilityLabel(
            String(
                localized: "Downloading the track…",
                comment:
                    "Spoken for the arc that turns around Play while the track about to play waits on the library's cloud."
            )
        )
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    override func layout() {
        super.layout()
        arc.frame = bounds
        let inset = Self.strokeWidth / 2
        arc.path = CGPath(
            ellipseIn: bounds.insetBy(dx: inset, dy: inset),
            transform: nil
        )
    }

    func show(_ waits: Bool) {
        isHidden = !waits
        if waits, arc.animation(forKey: Self.turnKey) == nil {
            let turn = CABasicAnimation(keyPath: "transform.rotation.z")
            turn.fromValue = 0
            turn.toValue = -2 * Double.pi
            turn.duration = 1
            turn.repeatCount = .infinity
            arc.add(turn, forKey: Self.turnKey)
        }
        else if !waits {
            arc.removeAnimation(forKey: Self.turnKey)
        }
    }
}
