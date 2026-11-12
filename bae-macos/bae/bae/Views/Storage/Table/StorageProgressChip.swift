import AppKit
import BaeKit
import SwiftUI

/// A storage chip for files on the move: its word on the transfer's tint,
/// filled left to right as far as the transfer has come. As wide as its word.
/// The fill is a native view that reads the release's latest progress each
/// display frame, so moving bytes run no SwiftUI update.
struct StorageProgressChip: View {
    let label: Text
    let kind: StorageTransferKind
    /// The release's stream; `nil` draws no fill, as for files only waiting
    /// their turn.
    let fill: ReleaseProgressSource?

    init(
        label: LocalizedStringKey,
        kind: StorageTransferKind,
        fill: ReleaseProgressSource?
    ) {
        self.init(text: Text(label), kind: kind, fill: fill)
    }

    /// A chip whose words are already in the person's language.
    init(
        verbatim label: String,
        kind: StorageTransferKind,
        fill: ReleaseProgressSource?
    ) {
        self.init(text: Text(verbatim: label), kind: kind, fill: fill)
    }

    private init(
        text: Text,
        kind: StorageTransferKind,
        fill: ReleaseProgressSource?
    ) {
        label = text
        self.kind = kind
        self.fill = fill
    }

    var body: some View {
        label
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

/// A chip's fill, read from its release's latest progress each display
/// frame while the release moves: the fill's width follows it, and its
/// accessibility value says the percent.
final class ChipFillNSView: NSView {
    private let bar = CALayer()
    private var fraction: Double = 0
    private lazy var uploads = FrameFollowing<BridgeReleaseUploadTransfer>(
        view: self
    ) { [weak self] transfer in self?.show(transfer?.bar.fraction) }
    private lazy var pins = FrameFollowing<BridgeDownloadTransferProgress>(
        view: self
    ) { [weak self] progress in self?.show(progress?.fraction) }
    private lazy var makeLocals = FrameFollowing<BridgeMakeLocalProgress>(
        view: self
    ) { [weak self] progress in self?.show(progress?.fraction) }

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

    func follow(_ fill: ReleaseProgressSource) {
        switch fill {
        case .upload(let source):
            pins.stop()
            makeLocals.stop()
            uploads.follow(source)
        case .pin(let source):
            uploads.stop()
            makeLocals.stop()
            pins.follow(source)
        case .makeLocal(let source):
            uploads.stop()
            pins.stop()
            makeLocals.follow(source)
        }
    }

    func stopFollowing() {
        uploads.stop()
        pins.stop()
        makeLocals.stop()
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
    let fill: ReleaseProgressSource
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
