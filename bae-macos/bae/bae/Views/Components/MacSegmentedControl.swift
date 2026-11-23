import AppKit
import BaeKit
import SwiftUI

/// The pill of choices the design draws for a few exclusive options: a `tile`
/// track 28 high, the chosen option on a raised fill. `off` dims the whole
/// control while none of its options can be chosen.
struct MacSegmentedControl: View {
    struct Segment: Equatable {
        let label: String
        let systemImage: String?
    }

    @Binding
    var selectedIndex: Int
    let segments: [Segment]
    var off = false
    /// The track fills the width it is given, as the field beside a label
    /// does, with the options at its start.
    var fillsWidth = false

    /// Where each segment is, in the control's own space, for the radio
    /// buttons that assistive technology reads.
    @State
    private var frames: [Int: CGRect] = [:]

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(segments.enumerated()), id: \.offset) {
                index,
                segment in
                let isSelected = index == selectedIndex
                Button {
                    selectedIndex = index
                } label: {
                    HStack(spacing: ThemeSpace.inline) {
                        if let image = segment.systemImage {
                            Image(systemName: image)
                                .themeIcon(.small)
                        }
                        Text(verbatim: segment.label)
                            .themeText(.body)
                    }
                    .padding(.horizontal, 14)
                    .frame(maxHeight: .infinity)
                    .background {
                        if isSelected {
                            Capsule().fill(Theme.hover)
                            Capsule().fill(Theme.hairlineStrong)
                        }
                    }
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accentFocusRing(cornerRadius: .infinity)
                .onGeometryChange(for: CGRect.self) {
                    $0.frame(in: .named(Self.space))
                } action: {
                    frames[index] = $0
                }
            }
        }
        .coordinateSpace(.named(Self.space))
        // Assistive technology reads the control as the radio group it is,
        // one radio button for each choice, each where its segment is.
        .accessibilityHidden(true)
        .overlay {
            MacSegmentedRadioGroup(
                labels: segments.map(\.label),
                frames: segments.indices.map { frames[$0] },
                selectedIndex: selectedIndex,
                isEnabled: !off,
                select: { selectedIndex = $0 }
            )
        }
        .padding(3)
        .frame(
            maxWidth: fillsWidth ? .infinity : nil,
            alignment: .leading
        )
        .frame(height: ThemeSize.hitTarget)
        .background(Theme.tile, in: Capsule())
        .fixedSize(horizontal: !fillsWidth, vertical: true)
        .allowsHitTesting(!off)
        .opacity(off ? ThemeOpacity.off : 1)
    }

    private static let space = "segmented-control"
}

/// The control's radio group for assistive technology: one radio button for
/// each segment, in the segment's frame. It takes no clicks.
private struct MacSegmentedRadioGroup: NSViewRepresentable {
    let labels: [String]
    let frames: [CGRect?]
    let selectedIndex: Int
    let isEnabled: Bool
    let select: (Int) -> Void

    func makeNSView(context: Context) -> MacSegmentedRadioGroupView {
        MacSegmentedRadioGroupView()
    }

    func updateNSView(_ view: MacSegmentedRadioGroupView, context: Context) {
        view.show(
            labels: labels,
            frames: frames,
            selectedIndex: selectedIndex,
            isEnabled: isEnabled,
            select: select
        )
    }
}

final class MacSegmentedRadioGroupView: NSView {
    private var radios: [MacSegmentedRadio] = []
    /// Each segment's frame once measured; a segment not measured yet has no
    /// radio button to read.
    fileprivate var frames: [CGRect?] = []
    fileprivate var selectedIndex = 0
    fileprivate var isEnabled = true
    fileprivate var select: (Int) -> Void = { _ in }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func show(
        labels: [String],
        frames: [CGRect?],
        selectedIndex: Int,
        isEnabled: Bool,
        select: @escaping (Int) -> Void
    ) {
        if radios.map(\.label) != labels {
            radios = labels.indices.map {
                MacSegmentedRadio(group: self, index: $0, label: labels[$0])
            }
        }
        self.frames = frames
        self.selectedIndex = selectedIndex
        self.isEnabled = isEnabled
        self.select = select
    }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? { .radioGroup }

    /// The group is what is off while no choice can be made; its radio
    /// buttons stay enabled, as the design draws them.
    override func isAccessibilityEnabled() -> Bool { isEnabled }

    override func accessibilityChildren() -> [Any]? { measuredRadios }

    override func accessibilityContents() -> [Any]? { measuredRadios }

    private var measuredRadios: [MacSegmentedRadio] {
        radios.indices.filter { frames[$0] != nil }.map { radios[$0] }
    }
}

/// One segment's radio button, reading the group for its place and state.
private final class MacSegmentedRadio: NSAccessibilityElement {
    private weak var group: MacSegmentedRadioGroupView?
    private let index: Int
    let label: String

    init(group: MacSegmentedRadioGroupView, index: Int, label: String) {
        self.group = group
        self.index = index
        self.label = label
        super.init()
    }

    override func accessibilityParent() -> Any? { group }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .radioButton
    }

    override func accessibilityLabel() -> String? { label }

    /// Enabled whatever the group is: the design draws the radio buttons of
    /// an off control enabled, under a group that is off.
    override func isAccessibilityEnabled() -> Bool { true }

    override func isAccessibilitySelected() -> Bool {
        let group = group
        let index = index
        return MainActor.assumeIsolated { group?.selectedIndex == index }
    }

    override func accessibilityValue() -> Any? {
        isAccessibilitySelected() ? 1 : 0
    }

    override func accessibilityFrame() -> NSRect {
        let group = group
        let index = index
        return MainActor.assumeIsolated {
            guard let group, let window = group.window,
                group.frames.indices.contains(index),
                let frame = group.frames[index]
            else { return .zero }
            return window.convertToScreen(group.convert(frame, to: nil))
        }
    }

    override func accessibilityPerformPress() -> Bool {
        let group = group
        let index = index
        return MainActor.assumeIsolated {
            guard let group, group.isEnabled else { return false }
            group.select(index)
            return true
        }
    }
}
