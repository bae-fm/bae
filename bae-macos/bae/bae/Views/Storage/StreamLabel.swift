import AppKit
import BaeKit
import SwiftUI

/// Which stream a `StreamLabel` follows, whatever its source.
enum StreamLabelKind: CaseIterable {
    case uploadRate, pinRate, exportPercent
}

/// A label that reads a fast stream's latest value once per display frame
/// while the stream is active, with no SwiftUI update. Its width is the
/// widest text the stream can show, so a changing value never asks SwiftUI
/// to lay the row out again.
final class StreamLabelNSView: NSTextField {
    private lazy var downloads = FrameFollowing<BridgeDownloadTransferProgress>(
        view: self
    ) { [weak self] progress in self?.showRate(progress?.throughputBps) }
    private lazy var percents = FrameFollowing<UInt8>(view: self) {
        [weak self] percent in
        self?.stringValue =
            percent.map { (Double($0) / 100).formatted(.percent) } ?? ""
    }
    private lazy var uploads = FrameFollowing<BridgeUploadTransfer>(
        view: self
    ) { [weak self] transfer in self?.showRate(transfer?.throughputBps) }

    init() {
        super.init(frame: .zero)
        isEditable = false
        isSelectable = false
        isBordered = false
        drawsBackground = false
        alignment = .left
        lineBreakMode = .byClipping
        textColor = NSColor(Theme.textSecondary)
        font = NSFont.monospacedDigitSystemFont(
            ofSize: ThemeText.body.nsFont.pointSize,
            weight: ThemeText.body.nsWeight
        )
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    /// The label only shows a value: a click on it is the click of the view
    /// it sits in.
    override func hitTest(_: NSPoint) -> NSView? { nil }

    func follow(_ stream: StreamLabel.Stream) {
        stopAllBut(stream)
        switch stream {
        case .uploadRate(let total):
            uploads.follow(total)
        case .pinRate(let source):
            downloads.follow(source)
        case .exportPercent(let source):
            percents.follow(source)
        }
    }

    /// Stop what the label followed before, unless it is what `stream`
    /// follows: a label is handed its stream again by each SwiftUI update.
    private func stopAllBut(_ stream: StreamLabel.Stream) {
        for kind in StreamLabelKind.allCases where kind != stream.kind {
            stop(kind)
        }
    }

    func stopFollowing() {
        for kind in StreamLabelKind.allCases {
            stop(kind)
        }
    }

    private func stop(_ kind: StreamLabelKind) {
        switch kind {
        case .uploadRate: uploads.stop()
        case .pinRate: downloads.stop()
        case .exportPercent: percents.stop()
        }
    }

    /// A zero or unknown rate shows nothing rather than a stalled speed.
    private func showRate(_ rate: UInt64?) {
        stringValue =
            rate.flatMap {
                $0 > 0 ? TransferThroughput.text(bytesPerSecond: $0) : nil
            } ?? ""
    }
}

/// SwiftUI's place for a stream's rate, percent or count: it hands the label
/// its stream, never a value, and sizes it for the widest text the stream
/// can show.
struct StreamLabel: NSViewRepresentable {
    /// The stream a footer label shows.
    enum Stream {
        /// How fast every file being sent goes together.
        case uploadRate(LatestSource<BridgeUploadTransfer>)
        /// How fast the pin queue's running release downloads.
        case pinRate(LatestSource<BridgeDownloadTransferProgress>)
        /// How far the export queue's running release has been written.
        case exportPercent(LatestSource<UInt8>)

        var kind: StreamLabelKind {
            switch self {
            case .uploadRate: .uploadRate
            case .pinRate: .pinRate
            case .exportPercent: .exportPercent
            }
        }
    }

    let stream: Stream

    func makeNSView(context _: Context) -> StreamLabelNSView {
        let view = StreamLabelNSView()
        view.follow(stream)
        return view
    }

    func updateNSView(_ view: StreamLabelNSView, context _: Context) {
        view.follow(stream)
    }

    static func dismantleNSView(_ view: StreamLabelNSView, coordinator _: ()) {
        view.stopFollowing()
    }

    func sizeThatFits(
        _: ProposedViewSize,
        nsView view: StreamLabelNSView,
        context _: Context
    ) -> CGSize? {
        let widest =
            switch stream {
            case .uploadRate, .pinRate:
                TransferThroughput.text(bytesPerSecond: 999_900_000)
            case .exportPercent:
                (1.0).formatted(.percent)
            }
        let font = view.font ?? .systemFont(ofSize: NSFont.systemFontSize)
        let size = (widest as NSString).size(withAttributes: [.font: font])
        return CGSize(width: ceil(size.width) + 4, height: ceil(size.height))
    }
}
