import AppKit
import BaeKit
import SwiftUI

/// A release transfer's readout: the bytes it holds of its size and how fast
/// it goes on one line, over a bar of the same bytes, all read from the
/// release's latest progress once per display frame while it moves. Moving
/// bytes run no SwiftUI update. Before the stream names the release the
/// labels are empty and the bar empty.
final class TransferReadoutNSView: NSView {
    /// The gap between the line of text and the bar.
    static let lineGap: CGFloat = 10

    private let bytesField = NSTextField(labelWithString: "")
    private let rateField = NSTextField(labelWithString: "")
    private let bar: ProgressTrackNSView
    private lazy var uploads = FrameFollowing<BridgeReleaseUploadTransfer>(
        view: self
    ) { [weak self] transfer in
        self?
            .show(
                bytes: transfer?.bar.text,
                rate: transfer.map(\.throughputBps),
                fraction: transfer?.bar.fraction
            )
    }
    private lazy var pins = FrameFollowing<BridgeDownloadTransferProgress>(
        view: self
    ) { [weak self] progress in
        self?
            .show(
                bytes: progress?.bytesText,
                rate: progress?.throughputBps,
                fraction: progress?.fraction
            )
    }

    var tint: NSColor {
        get { bar.accent }
        set { bar.accent = newValue }
    }

    init(tint: NSColor) {
        bar = ProgressTrackNSView(progress: 0, accent: tint)
        super.init(frame: .zero)
        let font = NSFont.monospacedDigitSystemFont(
            ofSize: ThemeText.detail.nsFont.pointSize,
            weight: ThemeText.detail.nsWeight
        )
        for field in [bytesField, rateField] {
            field.font = font
            field.textColor = .secondaryLabelColor
            field.lineBreakMode = .byTruncatingTail
            field.translatesAutoresizingMaskIntoConstraints = false
        }
        rateField.alignment = .right
        bytesField.setContentCompressionResistancePriority(
            .defaultLow,
            for: .horizontal
        )
        bar.translatesAutoresizingMaskIntoConstraints = false
        addSubview(bytesField)
        addSubview(rateField)
        addSubview(bar)
        NSLayoutConstraint.activate([
            bytesField.leadingAnchor.constraint(equalTo: leadingAnchor),
            bytesField.topAnchor.constraint(equalTo: topAnchor),
            rateField.trailingAnchor.constraint(equalTo: trailingAnchor),
            rateField.firstBaselineAnchor.constraint(
                equalTo: bytesField.firstBaselineAnchor
            ),
            rateField.leadingAnchor.constraint(
                greaterThanOrEqualTo: bytesField.trailingAnchor,
                constant: ThemeSpace.related
            ),
            bar.leadingAnchor.constraint(equalTo: leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: trailingAnchor),
            bar.topAnchor.constraint(
                equalTo: bytesField.bottomAnchor,
                constant: Self.lineGap
            ),
            bar.heightAnchor.constraint(
                equalToConstant: ProgressTrackNSView.trackHeight
            ),
            bar.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    /// The readout's height: a line of text, the gap and the bar.
    static var height: CGFloat {
        let font = ThemeText.detail.nsFont
        return ceil(font.ascender - font.descender + font.leading) + lineGap
            + ProgressTrackNSView.trackHeight
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: Self.height)
    }

    func follow(_ stream: ReleaseProgressSource) {
        switch stream {
        case .upload(let source):
            pins.stop()
            uploads.follow(source)
        case .pin(let source):
            uploads.stop()
            pins.follow(source)
        }
    }

    func stopFollowing() {
        uploads.stop()
        pins.stop()
    }

    /// A zero or unknown rate shows nothing rather than a stalled speed.
    private func show(bytes: String?, rate: UInt64?, fraction: Double?) {
        bytesField.stringValue = bytes ?? ""
        rateField.stringValue =
            rate.flatMap {
                $0 > 0 ? QueueSummary.throughputText(bytesPerSecond: $0) : nil
            } ?? ""
        bar.progress = fraction ?? 0
    }
}

/// SwiftUI's place for a release transfer's readout: it hands the readout
/// the release's stream and its tint, never a value.
struct TransferReadout: NSViewRepresentable {
    let stream: ReleaseProgressSource
    let kind: StorageTransferKind

    func makeNSView(context _: Context) -> TransferReadoutNSView {
        let view = TransferReadoutNSView(tint: NSColor(kind.color))
        view.follow(stream)
        return view
    }

    func updateNSView(_ view: TransferReadoutNSView, context _: Context) {
        view.tint = NSColor(kind.color)
        view.follow(stream)
    }

    static func dismantleNSView(
        _ view: TransferReadoutNSView,
        coordinator _: ()
    ) {
        view.stopFollowing()
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView _: TransferReadoutNSView,
        context _: Context
    ) -> CGSize? {
        CGSize(width: proposal.width ?? 0, height: TransferReadoutNSView.height)
    }
}
