import AppKit
import BaeKit
import SwiftUI

/// A label a fast stream sets straight: each value lands as the field's
/// text, with no SwiftUI update. Its width is the widest text the stream can
/// show, so a changing value never asks SwiftUI to lay the row out again.
final class StreamLabelNSView: NSTextField {
    private let releases = KeyedValueFollowing<BridgeDownloadTransferProgress>()
    private let releaseUploads = KeyedValueFollowing<
        BridgeReleaseUploadTransfer
    >()
    private let percents = KeyedValueFollowing<UInt8>()
    private let uploads = LatestValueFollowing<BridgeUploadTransfer>()

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
            releases.stop()
            percents.stop()
            uploads.follow(total) { [weak self] transfer in
                self?.showRate(transfer?.throughputBps)
            }
        case .uploadBytes(let source):
            uploads.stop()
            releases.stop()
            percents.stop()
            releaseUploads.follow(source) { [weak self] transfer in
                self?.stringValue = transfer?.bar.text ?? ""
            }
        case .pinRate(let source):
            uploads.stop()
            releaseUploads.stop()
            percents.stop()
            releases.follow(source) { [weak self] progress in
                self?.showRate(progress?.throughputBps)
            }
        case .exportPercent(let source):
            uploads.stop()
            releaseUploads.stop()
            releases.stop()
            percents.follow(source) { [weak self] percent in
                self?.stringValue =
                    percent.map {
                        (Double($0) / 100).formatted(.percent)
                    } ?? ""
            }
        }
    }

    func stopFollowing() {
        uploads.stop()
        releaseUploads.stop()
        releases.stop()
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

/// SwiftUI's place for a footer queue's rate or percent: it hands the label
/// its stream, never a value, and sizes it for the widest text the stream
/// can show.
struct StreamLabel: NSViewRepresentable {
    /// The stream a footer label shows.
    enum Stream {
        /// How fast every file being sent goes together.
        case uploadRate(LatestValue<BridgeUploadTransfer>)
        /// The bytes a release holds of its size while its files are sent.
        case uploadBytes(KeyedValueSource<BridgeReleaseUploadTransfer>)
        /// How fast the pin queue's running release downloads.
        case pinRate(KeyedValueSource<BridgeDownloadTransferProgress>)
        /// How far the export queue's running release has been written.
        case exportPercent(KeyedValueSource<UInt8>)
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
