import AppKit
import BaeKit
import SwiftUI

/// A bar set straight from a release transfer's keyed stream: each value
/// moves the bar's fill and runs no SwiftUI update. Before the stream names
/// the release the bar is empty.
final class StreamBarNSView: ProgressTrackNSView {
    private let uploads = KeyedValueFollowing<BridgeReleaseUploadTransfer>()
    private let pins = KeyedValueFollowing<BridgeDownloadTransferProgress>()

    func follow(_ stream: ReleaseTransferStream) {
        switch stream {
        case .upload(let source):
            pins.stop()
            uploads.follow(source) { [weak self] transfer in
                self?.progress = transfer?.bar.fraction ?? 0
            }
        case .pin(let source):
            uploads.stop()
            pins.follow(source) { [weak self] progress in
                self?.progress = progress?.fraction ?? 0
            }
        }
    }

    func stopFollowing() {
        uploads.stop()
        pins.stop()
    }
}

/// SwiftUI's place for a release transfer's bar: it hands the bar its stream
/// and fill, never a value. With no tint the bar fills with the window's
/// accent.
struct StreamBar: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let stream: ReleaseTransferStream
    var tint: Color?

    func makeNSView(context _: Context) -> StreamBarNSView {
        let view = StreamBarNSView(progress: 0, accent: fill)
        view.follow(stream)
        return view
    }

    func updateNSView(_ view: StreamBarNSView, context _: Context) {
        view.accent = fill
        view.follow(stream)
    }

    static func dismantleNSView(_ view: StreamBarNSView, coordinator _: ()) {
        view.stopFollowing()
    }

    private var fill: NSColor {
        NSColor(tint ?? accent.color(in: colorScheme))
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView _: StreamBarNSView,
        context _: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? 0,
            height: ProgressTrackNSView.trackHeight
        )
    }
}
