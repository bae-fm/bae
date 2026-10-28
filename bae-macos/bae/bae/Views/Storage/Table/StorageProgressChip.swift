import BaeKit
import SwiftUI

/// A storage chip for files on the move: its word on the transfer's tint,
/// filled left to right as far as the transfer has come. As wide as its word.
struct StorageProgressChip: View {
    let label: LocalizedStringKey
    let kind: StorageTransferKind
    /// How far the transfer has come, 0...1; `nil` draws no fill, as for
    /// files only waiting their turn.
    let fraction: Double?

    var body: some View {
        if let fraction {
            chip.accessibilityValue(
                Text(fraction, format: .percent.precision(.fractionLength(0)))
            )
        }
        else {
            chip
        }
    }

    private var chip: some View {
        Text(label)
            .themeText(.chip)
            .lineLimit(1)
            .foregroundStyle(.primary)
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background {
                GeometryReader { proxy in
                    ZStack(alignment: .leading) {
                        kind.color.opacity(ThemeOpacity.tint)
                        if let fraction {
                            kind.color.opacity(ThemeOpacity.tintStrong)
                                .frame(width: proxy.size.width * fraction)
                        }
                    }
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
    }
}

#if DEBUG
    #Preview("Progress chips") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            StorageProgressChip(
                label: "Uploading",
                kind: .upload,
                fraction: 0.62
            )
            StorageProgressChip(
                label: "Uploading",
                kind: .upload,
                fraction: nil
            )
            StorageProgressChip(label: "Pinning", kind: .pin, fraction: 0.35)
            StorageProgressChip(label: "Pinning", kind: .pin, fraction: nil)
        }
        .padding()
    }
#endif
