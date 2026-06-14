import AppKit
import SwiftUI

/// SwiftUI wrapper around `NSSegmentedControl` because SwiftUI's own
/// `.pickerStyle(.segmented)` silently drops images — we need per-segment
/// image + label to show a playing indicator.
struct NativeSegmentedControl: NSViewRepresentable {
    struct Segment: Equatable {
        let label: String
        let systemImage: String?
    }

    @Binding
    var selectedIndex: Int
    let segments: [Segment]

    func makeNSView(context: Context) -> NSSegmentedControl {
        let control = NSSegmentedControl()
        control.segmentStyle = .automatic
        control.trackingMode = .selectOne
        control.target = context.coordinator
        control.action = #selector(Coordinator.selectionChanged(_:))
        control.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        control.setContentCompressionResistancePriority(
            .defaultHigh,
            for: .horizontal
        )
        return control
    }

    func updateNSView(_ nsView: NSSegmentedControl, context: Context) {
        context.coordinator.selectedIndex = $selectedIndex

        if nsView.segmentCount != segments.count {
            nsView.segmentCount = segments.count
        }
        for (index, segment) in segments.enumerated() {
            nsView.setLabel(segment.label, forSegment: index)
            let image = segment.systemImage.flatMap(Self.paddedSymbol(named:))
            nsView.setImage(image, forSegment: index)
            if image != nil {
                nsView.setImageScaling(
                    .scaleProportionallyDown,
                    forSegment: index
                )
            }
        }
        if selectedIndex >= 0, selectedIndex < segments.count,
            nsView.selectedSegment != selectedIndex
        {
            nsView.selectedSegment = selectedIndex
        }
    }

    func sizeThatFits(
        _: ProposedViewSize,
        nsView: NSSegmentedControl,
        context _: Context
    )
        -> CGSize?
    {
        nsView.intrinsicContentSize
    }

    /// NSSegmentedControl draws image flush against the label. Wrap the symbol
    /// in a larger transparent canvas so there's a gap between icon and text.
    private static func paddedSymbol(named name: String) -> NSImage? {
        guard
            let symbol = NSImage(
                systemSymbolName: name,
                accessibilityDescription: nil
            )
        else {
            return nil
        }
        let base = symbol.size
        let trailingPadding: CGFloat = 5
        let canvas = NSImage(
            size: NSSize(
                width: base.width + trailingPadding,
                height: base.height
            ),
            flipped: false
        ) { _ in
            symbol.draw(
                in: NSRect(x: 0, y: 0, width: base.width, height: base.height)
            )
            return true
        }
        canvas.isTemplate = symbol.isTemplate
        return canvas
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(selectedIndex: $selectedIndex)
    }

    @MainActor
    final class Coordinator: NSObject {
        var selectedIndex: Binding<Int>

        init(selectedIndex: Binding<Int>) {
            self.selectedIndex = selectedIndex
        }

        @objc
        func selectionChanged(_ sender: NSSegmentedControl) {
            selectedIndex.wrappedValue = sender.selectedSegment
        }
    }
}
