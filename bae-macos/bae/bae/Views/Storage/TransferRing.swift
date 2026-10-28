import AppKit
import BaeKit
import SwiftUI

/// A ring that fills as far as one file's transfer has come, set straight
/// from the file's keyed stream: a step of bytes moves the ring's stroke and
/// runs no SwiftUI update. Before the stream names the file the ring is
/// empty.
final class TransferRingNSView: NSView {
    /// The ring's width.
    static let lineWidth: CGFloat = 1.75

    private let track = CAShapeLayer()
    private let fill = CAShapeLayer()
    private let following = KeyedValueFollowing<Double>()

    var color: NSColor {
        didSet { needsDisplay = true }
    }

    init(color: NSColor) {
        self.color = color
        super.init(frame: .zero)
        wantsLayer = true
        for ring in [track, fill] {
            ring.fillColor = nil
            ring.lineWidth = Self.lineWidth
            layer?.addSublayer(ring)
        }
        fill.lineCap = .round
        fill.strokeEnd = 0
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            track.strokeColor = NSColor(Theme.hairline).cgColor
            fill.strokeColor = color.cgColor
        }
    }

    override func layout() {
        super.layout()
        let inset = bounds.insetBy(dx: Self.lineWidth, dy: Self.lineWidth)
        // From the top, clockwise, as a ring fills.
        let path = CGMutablePath()
        let radius = min(inset.width, inset.height) / 2
        path.addArc(
            center: CGPoint(x: inset.midX, y: inset.midY),
            radius: radius,
            startAngle: .pi / 2,
            endAngle: .pi / 2 - 2 * .pi,
            clockwise: true
        )
        for ring in [track, fill] {
            ring.frame = bounds
            ring.path = path
        }
    }

    /// Follow `source`, filling as far as each share it tells.
    func follow(_ source: KeyedValueSource<Double>) {
        following.follow(source) { [weak self] fraction in
            self?.show(fraction)
        }
    }

    func stopFollowing() {
        following.stop()
    }

    private func show(_ fraction: Double?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fill.strokeEnd = CGFloat(min(max(fraction ?? 0, 0), 1))
        CATransaction.commit()
    }
}

/// SwiftUI's place for a file's transfer ring: it hands the ring the file's
/// stream and its colour, never a value.
struct TransferRing: NSViewRepresentable {
    let kind: StorageTransferKind
    let source: KeyedValueSource<Double>

    func makeNSView(context _: Context) -> TransferRingNSView {
        let view = TransferRingNSView(color: NSColor(kind.color))
        view.follow(source)
        return view
    }

    func updateNSView(_ view: TransferRingNSView, context _: Context) {
        view.color = NSColor(kind.color)
        view.follow(source)
    }

    static func dismantleNSView(_ view: TransferRingNSView, coordinator _: ()) {
        view.stopFollowing()
    }
}
