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
            MacRadioGroupAccessibility(
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
