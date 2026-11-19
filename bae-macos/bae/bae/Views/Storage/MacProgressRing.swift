import BaeKit
import SwiftUI

/// A ring that fills as far as a stream of fractions has come, in `color`.
/// SwiftUI hands the ring its stream and colour, never a value.
struct MacProgressRing: NSViewRepresentable {
    let color: Color
    let source: LatestSource<Double>

    func makeNSView(context _: Context) -> TransferRingNSView {
        let view = TransferRingNSView(color: NSColor(color))
        view.follow(source)
        return view
    }

    func updateNSView(_ view: TransferRingNSView, context _: Context) {
        view.color = NSColor(color)
        view.follow(source)
    }

    static func dismantleNSView(_ view: TransferRingNSView, coordinator _: ()) {
        view.stopFollowing()
    }
}
