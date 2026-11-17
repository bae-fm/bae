import AppKit
import BaeKit
import SwiftUI

/// The design system's progress track for a share of bytes. The bar reads its
/// key's latest value once per display frame while the stream sends new values
/// for the key, with no SwiftUI update. Before the stream holds the key, the
/// bar is empty.
final class ShareBarNSView: ProgressTrackNSView {
    private lazy var following = FrameFollowing<Double>(view: self) {
        [weak self] share in
        self?.progress = share ?? 0
    }

    func follow(_ share: LatestSource<Double>) {
        following.follow(share)
    }

    func stopFollowing() {
        following.stop()
    }
}

/// SwiftUI's view of a transfer line's bar: it gives the bar its key of the
/// stream and the accent the person chose, never a value.
struct ShareBar: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let share: LatestSource<Double>

    func makeNSView(context _: Context) -> ShareBarNSView {
        let view = ShareBarNSView(progress: 0, accent: fill)
        view.follow(share)
        return view
    }

    func updateNSView(_ view: ShareBarNSView, context _: Context) {
        view.accent = fill
        view.follow(share)
    }

    static func dismantleNSView(_ view: ShareBarNSView, coordinator _: ()) {
        view.stopFollowing()
    }

    private var fill: NSColor {
        NSColor(accent.color(in: colorScheme))
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView _: ShareBarNSView,
        context _: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? 0,
            height: ProgressTrackNSView.trackHeight
        )
    }
}
