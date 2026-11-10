import AppKit
import BaeKit
import SwiftUI

/// Which stream a `StreamLabel` follows, whatever its source.
enum StreamLabelKind: CaseIterable {
    case uploadRate, uploadBytes, pinRate, exportPercent
    case scanFoundCount, tabCount
}

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
    private lazy var tabCounts = FrameFollowing<UInt32>(view: self) {
        [weak self] count in
        self?.stringValue = count.map { $0.formatted() } ?? ""
    }
    private lazy var foundCounts = FrameFollowing<UInt64>(view: self) {
        [weak self] count in
        self?.stringValue =
            count.map { localizedCoreString("ui.import.scan.found", Int($0)) }
            ?? ""
    }

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

    /// The label only shows a value: a click on it is the click of the view
    /// it sits in, such as the tab a count belongs to.
    override func hitTest(_: NSPoint) -> NSView? { nil }

    func follow(_ stream: StreamLabel.Stream) {
        stopAllBut(stream)
        switch stream {
        case .uploadRate(let total):
            uploads.follow(total)
        case .uploadBytes(let source):
            releaseUploads.follow(source)
        case .pinRate(let source):
            downloads.follow(source)
        case .exportPercent(let source):
            percents.follow(source)
        case .scanFoundCount(let source):
            alignment = .right
            foundCounts.follow(source)
        case .tabCount(let source, let tone):
            alignment = .center
            textColor = NSColor(tone.color)
            font = Self.chipFont
            tabCounts.follow(source)
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
        case .uploadBytes: releaseUploads.stop()
        case .pinRate: downloads.stop()
        case .exportPercent: percents.stop()
        case .scanFoundCount: foundCounts.stop()
        case .tabCount: tabCounts.stop()
        }
    }

    /// The font a chip's text is set in, with digits of one width.
    static var chipFont: NSFont {
        NSFont.monospacedDigitSystemFont(
            ofSize: ThemeText.chip.nsFont.pointSize,
            weight: ThemeText.chip.nsWeight
        )
    }

    /// A zero or unknown rate shows nothing rather than a stalled speed.
    private func showRate(_ rate: UInt64?) {
        stringValue =
            rate.flatMap {
                $0 > 0 ? TransferThroughput.text(bytesPerSecond: $0) : nil
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
        /// How many folders the scan of one watched folder has stored.
        case scanFoundCount(LatestSource<UInt64>)
        /// How many entries one tab of the import list holds, as a chip of
        /// `tone` shows it.
        case tabCount(LatestSource<UInt32>, tone: StatusTone)

        var kind: StreamLabelKind {
            switch self {
            case .uploadRate: .uploadRate
            case .uploadBytes: .uploadBytes
            case .pinRate: .pinRate
            case .exportPercent: .exportPercent
            case .scanFoundCount: .scanFoundCount
            case .tabCount: .tabCount
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
            case .uploadBytes:
                BridgeUploadBar(
                    bytesDone: 999_900_000_000,
                    bytesTotal: 999_900_000_000
                )
                .text
            case .exportPercent:
                (1.0).formatted(.percent)
            case .scanFoundCount:
                localizedCoreString("ui.import.scan.found", 9_999_999)
            case .tabCount:
                (99_999 as UInt32).formatted()
            }
        let font = view.font ?? .systemFont(ofSize: NSFont.systemFontSize)
        let size = (widest as NSString).size(withAttributes: [.font: font])
        return CGSize(width: ceil(size.width) + 4, height: ceil(size.height))
    }
}
