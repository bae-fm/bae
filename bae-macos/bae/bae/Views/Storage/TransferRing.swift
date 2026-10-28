import AppKit
import BaeKit
import SwiftUI

/// A ring that fills as far as one file's transfer has come, read from the
/// file's latest share once per display frame while the file is in flight:
/// moving bytes run no SwiftUI update and no main-thread work of their own.
/// Before the stream names the file the ring is empty.
final class TransferRingNSView: NSView {
    /// The ring's width.
    static let lineWidth: CGFloat = 1.75

    private let track = CAShapeLayer()
    private let fill = CAShapeLayer()
    private lazy var following = FrameFollowing<Double>(view: self) {
        [weak self] fraction in self?.show(fraction)
    }

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

    /// Read `source` each frame while its file is in flight.
    func follow(_ source: LatestSource<Double>) {
        following.follow(source)
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
    let source: LatestSource<Double>

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
