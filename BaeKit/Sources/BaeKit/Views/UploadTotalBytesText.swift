import SwiftUI

#if os(macOS)
    import AppKit

    private typealias PlatformLabel = NSTextField
#else
    import UIKit

    private typealias PlatformLabel = UILabel
#endif

/// The bytes the files being sent have moved together, as "3 MB of 221.2
/// MB", set straight from the total's stream: a step of bytes sets the
/// label's text and runs no SwiftUI update. It takes no room while no bytes
/// are counted, and asks for room again only as that changes.
public struct UploadTotalBytesText: View {
    private let total: LatestValue<BridgeUploadTransfer>

    public init(total: LatestValue<BridgeUploadTransfer>) {
        self.total = total
    }

    public var body: some View {
        Representable(total: total)
    }
}

/// A label that sets itself from the total's stream.
@MainActor
private final class UploadTotalBytesLabel {
    let label: PlatformLabel
    private let following = LatestValueFollowing<BridgeUploadTransfer>()

    init() {
        #if os(macOS)
            label = NSTextField(labelWithString: "")
            label.textColor = .secondaryLabelColor
            label.font = .monospacedDigitSystemFont(
                ofSize: ThemeText.detail.nsFont.pointSize,
                weight: ThemeText.detail.nsWeight
            )
        #else
            label = UILabel()
            label.textColor = .secondaryLabel
            label.font = .monospacedDigitSystemFont(
                ofSize: UIFont.smallSystemFontSize,
                weight: .regular
            )
        #endif
    }

    func follow(_ total: LatestValue<BridgeUploadTransfer>) {
        following.follow(total) { [weak self] transfer in
            self?.show(transfer?.bar)
        }
    }

    func stop() {
        following.stop()
    }

    private func show(_ bar: BridgeUploadBar?) {
        let text = bar.flatMap { $0.bytesTotal > 0 ? $0.text : nil } ?? ""
        let shown = !currentText.isEmpty
        #if os(macOS)
            label.stringValue = text
        #else
            label.text = text
        #endif
        if shown != !text.isEmpty {
            label.invalidateIntrinsicContentSize()
        }
    }

    private var currentText: String {
        #if os(macOS)
            label.stringValue
        #else
            label.text ?? ""
        #endif
    }

    var size: CGSize {
        currentText.isEmpty ? .zero : label.intrinsicContentSize
    }
}

#if os(macOS)
    private struct Representable: NSViewRepresentable {
        let total: LatestValue<BridgeUploadTransfer>

        func makeCoordinator() -> UploadTotalBytesLabel {
            UploadTotalBytesLabel()
        }

        func makeNSView(context: Context) -> NSTextField {
            context.coordinator.follow(total)
            return context.coordinator.label
        }

        func updateNSView(_: NSTextField, context: Context) {
            context.coordinator.follow(total)
        }

        static func dismantleNSView(
            _: NSTextField,
            coordinator: UploadTotalBytesLabel
        ) {
            coordinator.stop()
        }

        func sizeThatFits(
            _: ProposedViewSize,
            nsView _: NSTextField,
            context: Context
        ) -> CGSize? {
            context.coordinator.size
        }
    }
#else
    private struct Representable: UIViewRepresentable {
        let total: LatestValue<BridgeUploadTransfer>

        func makeCoordinator() -> UploadTotalBytesLabel {
            UploadTotalBytesLabel()
        }

        func makeUIView(context: Context) -> UILabel {
            context.coordinator.follow(total)
            return context.coordinator.label
        }

        func updateUIView(_: UILabel, context: Context) {
            context.coordinator.follow(total)
        }

        static func dismantleUIView(
            _: UILabel,
            coordinator: UploadTotalBytesLabel
        ) {
            coordinator.stop()
        }

        func sizeThatFits(
            _: ProposedViewSize,
            uiView _: UILabel,
            context: Context
        ) -> CGSize? {
            context.coordinator.size
        }
    }
#endif
