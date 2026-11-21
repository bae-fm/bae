import BaeKit
import SwiftUI

/// A ring that fills as far as a stream of fractions has come, in the
/// window's accent. SwiftUI hands the ring its stream and colour, never a
/// value.
struct MacProgressRing: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let source: LatestSource<Double>

    func makeNSView(context _: Context) -> TransferRingNSView {
        let view = TransferRingNSView(color: color)
        view.follow(source)
        return view
    }

    func updateNSView(_ view: TransferRingNSView, context _: Context) {
        view.color = color
        view.follow(source)
    }

    private var color: NSColor {
        NSColor(accent.color(in: colorScheme))
    }

    static func dismantleNSView(_ view: TransferRingNSView, coordinator _: ()) {
        view.stopFollowing()
    }
}
