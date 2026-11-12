import BaeKit
import SwiftUI

/// The pill of choices the design draws for a few exclusive options: a `tile`
/// track 28 high, the chosen option on a raised fill.
struct MacSegmentedControl: View {
    struct Segment: Equatable {
        let label: String
        let systemImage: String?
    }

    @Binding
    var selectedIndex: Int
    let segments: [Segment]

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
                .accessibilityAddTraits(isSelected ? .isSelected : [])
            }
        }
        .padding(3)
        .frame(height: ThemeSize.hitTarget)
        .background(Theme.tile, in: Capsule())
        .fixedSize()
    }
}
