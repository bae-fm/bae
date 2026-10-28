import AppKit
import BaeKit
import SwiftUI

/// A storage chip for files on the move: its word on the transfer's tint,
/// filled left to right as far as the transfer has come. As wide as its word.
/// The fill is a native view that sets itself from the release's stream, so
/// moving bytes run no SwiftUI update.
struct StorageProgressChip: View {
    let label: LocalizedStringKey
    let kind: StorageTransferKind
    /// The release's stream; `nil` draws no fill, as for files only waiting
    /// their turn.
    let fill: ReleaseTransferStream?

    var body: some View {
        Text(label)
            .themeText(.chip)
            .lineLimit(1)
            .foregroundStyle(.primary)
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background {
                ZStack(alignment: .leading) {
                    kind.color.opacity(ThemeOpacity.tint)
                    if let fill {
                        ChipFillView(
                            fill: fill,
                            color: kind.color.opacity(ThemeOpacity.tintStrong)
                        )
                    }
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
    }
}

/// A chip's fill, set straight from its release's stream: the fill's width
/// follows each value as it lands, and its accessibility value says the
/// percent.
final class ChipFillNSView: NSView {
    private let bar = CALayer()
    private var fraction: Double = 0
    private let uploads = KeyedValueFollowing<BridgeReleaseUploadTransfer>()
    private let pins = KeyedValueFollowing<BridgeDownloadTransferProgress>()

    var color: NSColor {
        didSet { needsDisplay = true }
    }

    init(color: NSColor) {
        self.color = color
        super.init(frame: .zero)
        wantsLayer = true
        layer?.addSublayer(bar)
        setAccessibilityElement(false)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            bar.backgroundColor = color.cgColor
        }
    }

    override func layout() {
        super.layout()
        place()
    }

    func follow(_ fill: ReleaseTransferStream) {
        switch fill {
        case .upload(let source):
            pins.stop()
            uploads.follow(source) { [weak self] transfer in
                self?.show(transfer?.bar.fraction)
            }
        case .pin(let source):
            uploads.stop()
            pins.follow(source) { [weak self] progress in
                self?.show(progress?.fraction)
            }
        }
    }

    func stopFollowing() {
        uploads.stop()
        pins.stop()
    }

    private func show(_ fraction: Double?) {
        self.fraction = min(max(fraction ?? 0, 0), 1)
        setAccessibilityValue(
            fraction.map {
                $0.formatted(.percent.precision(.fractionLength(0)))
            }
        )
        place()
    }

    private func place() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        bar.frame = CGRect(
            x: 0,
            y: 0,
            width: bounds.width * fraction,
            height: bounds.height
        )
        CATransaction.commit()
    }
}

/// SwiftUI's place for a chip's fill: it hands the fill its stream and
/// colour, never a value.
private struct ChipFillView: NSViewRepresentable {
    let fill: ReleaseTransferStream
    let color: Color

    func makeNSView(context _: Context) -> ChipFillNSView {
        let view = ChipFillNSView(color: NSColor(color))
        view.follow(fill)
        return view
    }

    func updateNSView(_ view: ChipFillNSView, context _: Context) {
        view.color = NSColor(color)
        view.follow(fill)
    }

    static func dismantleNSView(_ view: ChipFillNSView, coordinator _: ()) {
        view.stopFollowing()
    }
}

#if DEBUG
    #Preview("Progress chips") {
        let outbox = PreviewData.outboxStore()
        let downloads = PreviewData.downloadStore()
        return VStack(alignment: .leading, spacing: ThemeSpace.related) {
            StorageProgressChip(
                label: "Uploading",
                kind: .upload,
                fill: .upload(outbox.releaseTransfer(forRelease: "rel-row-1"))
            )
            StorageProgressChip(label: "Uploading", kind: .upload, fill: nil)
            StorageProgressChip(
                label: "Pinning",
                kind: .pin,
                fill: .pin(downloads.releaseTransfer(forRelease: "rel-row-5"))
            )
            StorageProgressChip(label: "Pinning", kind: .pin, fill: nil)
        }
        .padding()
    }
#endif
