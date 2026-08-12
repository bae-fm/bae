import BaeKit
import SwiftUI

/// The Pending / Done / Skipped tab bar above the candidate list.
/// Three equal segments, each a label plus a count badge from core's
/// `BridgeTriageTabCounts` — never an array length, which drifts the moment a
/// filter is applied.
struct TriageTabBar: View {
    @Binding
    var activeTab: BridgeTriageTab
    let counts: BridgeTriageTabCounts

    var body: some View {
        HStack(spacing: 4) {
            segment(.pending, "Pending", Int(counts.pending))
            segment(.done, "Done", Int(counts.done))
            segment(.skipped, "Skipped", Int(counts.skipped))
        }
    }

    private func segment(
        _ tab: BridgeTriageTab,
        _ label: LocalizedStringKey,
        _ count: Int
    ) -> some View {
        let isActive = activeTab == tab
        return Button {
            activeTab = tab
        } label: {
            HStack(spacing: 4) {
                Text(label)
                    .font(.system(size: 12.5, weight: .semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                Text(verbatim: count.formatted())
                    .font(.system(size: 11, weight: .semibold))
                    .monospacedDigit()
                    // A squeezed badge must overflow, never stack its digits.
                    .fixedSize()
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(
                        Capsule()
                            .fill(
                                isActive
                                    ? Theme.accent.opacity(0.28)
                                    : Color.secondary.opacity(0.15)
                            )
                    )
            }
            .foregroundStyle(isActive ? Theme.accent : Color.secondary)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(isActive ? Theme.accent.opacity(0.14) : Color.clear)
            )
        }
        .buttonStyle(.plain)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Tab bar") {
        TriageTabBar(
            activeTab: .constant(.pending),
            counts: BridgeTriageTabCounts(
                pending: 112,
                done: 3,
                skipped: 41
            )
        )
        .padding()
        .frame(width: 320)
        .windowBackground()
    }
#endif
