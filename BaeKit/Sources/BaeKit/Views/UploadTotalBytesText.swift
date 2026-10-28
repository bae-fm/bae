#if os(macOS)
    import AppKit
    import SwiftUI

    /// The bytes the files being sent have moved together, as "3 MB of
    /// 221.2 MB", read from the latest total once per display frame while
    /// files are being sent: moving bytes run no SwiftUI update. It takes no
    /// room while no bytes are counted, and asks for room again only as that
    /// changes.
    public struct UploadTotalBytesText: View {
        private let total: LatestSource<BridgeUploadTransfer>

        public init(total: LatestSource<BridgeUploadTransfer>) {
            self.total = total
        }

        public var body: some View {
            Representable(total: total)
        }
    }

    /// The label, which reads the total on its own display frames.
    private final class UploadTotalBytesField: NSTextField {
        private lazy var following = FrameFollowing<BridgeUploadTransfer>(
            view: self
        ) { [weak self] transfer in self?.show(transfer?.bar) }

        init() {
            super.init(frame: .zero)
            isEditable = false
            isSelectable = false
            isBordered = false
            drawsBackground = false
            textColor = .secondaryLabelColor
            font = .monospacedDigitSystemFont(
                ofSize: ThemeText.detail.nsFont.pointSize,
                weight: ThemeText.detail.nsWeight
            )
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError()
        }

        func follow(_ total: LatestSource<BridgeUploadTransfer>) {
            following.follow(total)
        }

        func stopFollowing() {
            following.stop()
        }

        /// The room the label takes: none while it counts no bytes.
        var shownSize: CGSize {
            stringValue.isEmpty ? .zero : intrinsicContentSize
        }

        private func show(_ bar: BridgeUploadBar?) {
            let text = bar.flatMap { $0.bytesTotal > 0 ? $0.text : nil } ?? ""
            let wasShown = !stringValue.isEmpty
            stringValue = text
            if wasShown != !text.isEmpty {
                invalidateIntrinsicContentSize()
            }
        }
    }

    private struct Representable: NSViewRepresentable {
        let total: LatestSource<BridgeUploadTransfer>

        func makeNSView(context _: Context) -> UploadTotalBytesField {
            let view = UploadTotalBytesField()
            view.follow(total)
            return view
        }

        func updateNSView(_ view: UploadTotalBytesField, context _: Context) {
            view.follow(total)
        }

        static func dismantleNSView(
            _ view: UploadTotalBytesField,
            coordinator _: ()
        ) {
            view.stopFollowing()
        }

        func sizeThatFits(
            _: ProposedViewSize,
            nsView view: UploadTotalBytesField,
            context _: Context
        ) -> CGSize? {
            view.shownSize
        }
    }
#endif
