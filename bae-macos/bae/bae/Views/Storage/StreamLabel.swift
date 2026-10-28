import AppKit
import BaeKit
import SwiftUI

/// A label that reads a fast stream's latest value once per display frame
/// while the stream is active, with no SwiftUI update. Its width is the
/// widest text the stream can show, so a changing value never asks SwiftUI
/// to lay the row out again.
final class StreamLabelNSView: NSTextField {
    private lazy var downloads = FrameFollowing<BridgeDownloadTransferProgress>(
        view: self
    ) { [weak self] progress in self?.showRate(progress?.throughputBps) }
    private lazy var releaseUploads = FrameFollowing<
        BridgeReleaseUploadTransfer
    >(view: self) { [weak self] transfer in
        self?.stringValue = transfer?.bar.text ?? ""
    }
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
        textColor = .secondaryLabelColor
        font = NSFont.monospacedDigitSystemFont(
            ofSize: ThemeText.body.nsFont.pointSize,
            weight: ThemeText.body.nsWeight
        )
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    func follow(_ stream: StreamLabel.Stream) {
        switch stream {
        case .uploadRate(let total):
            releaseUploads.stop()
            downloads.stop()
            percents.stop()
            uploads.follow(total)
        case .uploadBytes(let source):
            uploads.stop()
            downloads.stop()
            percents.stop()
            releaseUploads.follow(source)
        case .pinRate(let source):
            uploads.stop()
            releaseUploads.stop()
            percents.stop()
            downloads.follow(source)
        case .exportPercent(let source):
            uploads.stop()
            releaseUploads.stop()
            downloads.stop()
            percents.follow(source)
        }
    }

    func stopFollowing() {
        uploads.stop()
        releaseUploads.stop()
        downloads.stop()
        percents.stop()
    }

    /// A zero or unknown rate shows nothing rather than a stalled speed.
    private func showRate(_ rate: UInt64?) {
        stringValue =
            rate.flatMap {
                $0 > 0 ? QueueSummary.throughputText(bytesPerSecond: $0) : nil
            } ?? ""
    }
}

/// SwiftUI's place for a stream's rate, bytes or percent: it hands the label
/// its stream, never a value, and sizes it for the widest text the stream
/// can show.
struct StreamLabel: NSViewRepresentable {
    /// The stream a footer label shows.
    enum Stream {
        /// How fast every file being sent goes together.
        case uploadRate(LatestSource<BridgeUploadTransfer>)
        /// The bytes a release holds of its size while its files are sent.
        case uploadBytes(LatestSource<BridgeReleaseUploadTransfer>)
        /// How fast the pin queue's running release downloads.
        case pinRate(LatestSource<BridgeDownloadTransferProgress>)
        /// How far the export queue's running release has been written.
        case exportPercent(LatestSource<UInt8>)
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
                QueueSummary.throughputText(bytesPerSecond: 999_900_000)
            case .uploadBytes:
                BridgeUploadBar(
                    bytesDone: 999_900_000_000,
                    bytesTotal: 999_900_000_000
                )
                .text
            case .exportPercent:
                (1.0).formatted(.percent)
            }
        let font = view.font ?? .systemFont(ofSize: NSFont.systemFontSize)
        let size = (widest as NSString).size(withAttributes: [.font: font])
        return CGSize(width: ceil(size.width) + 4, height: ceil(size.height))
    }
}
