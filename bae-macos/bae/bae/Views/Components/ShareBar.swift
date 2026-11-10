import AppKit
import BaeKit
import SwiftUI

/// The system's small progress bar for a share of bytes, which reads its
/// key's latest value once per display frame while the key moves, with no
/// SwiftUI update. Before the stream holds the key the bar is empty.
final class ShareBarNSView: NSProgressIndicator {
    private lazy var following = FrameFollowing<Double>(view: self) {
        [weak self] share in
        self?.doubleValue = share ?? 0
    }

    init() {
        super.init(frame: .zero)
        style = .bar
        controlSize = .small
        isIndeterminate = false
        minValue = 0
        maxValue = 1
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    func follow(_ share: LatestSource<Double>) {
        following.follow(share)
    }

    func stopFollowing() {
        following.stop()
    }
}

/// SwiftUI's place for a transfer line's bar: it hands the bar its key of the
/// stream, never a value.
struct ShareBar: NSViewRepresentable {
    let share: LatestSource<Double>

    func makeNSView(context _: Context) -> ShareBarNSView {
        let view = ShareBarNSView()
        view.follow(share)
        return view
    }

    func updateNSView(_ view: ShareBarNSView, context _: Context) {
        view.follow(share)
    }

    static func dismantleNSView(_ view: ShareBarNSView, coordinator _: ()) {
        view.stopFollowing()
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: ShareBarNSView,
        context _: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? 0,
            height: nsView.intrinsicContentSize.height
        )
    }
}
