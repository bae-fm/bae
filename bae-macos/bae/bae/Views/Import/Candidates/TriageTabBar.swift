import BaeKit
import SwiftUI

/// The Found / Imported / Skipped tab bar above the candidate list, with
/// counts from core's `BridgeTriageTabCounts` so a filter does not change them.
struct TriageTabBar: View {
    @Binding
    var activeTab: BridgeTriageTab
    let counts: BridgeTriageTabCounts

    var body: some View {
        HStack(spacing: 4) {
            segment(.pending, Int(counts.pending))
            segment(.done, Int(counts.done))
            segment(.skipped, Int(counts.skipped))
        }
    }

    private func segment(
        _ tab: BridgeTriageTab,
        _ count: Int
    ) -> some View {
        let isActive = activeTab == tab
        return Button {
            activeTab = tab
        } label: {
            HStack(spacing: 4) {
                Text(verbatim: CandidateFolderLine.label(for: tab))
                    .themeText(.strong)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                Text(verbatim: count.formatted())
                    .themeText(.chip)
                    .monospacedDigit()
                    // A squeezed badge must overflow, never stack its digits.
                    .fixedSize()
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(
                        Capsule()
                            .fill(
                                isActive
                                    ? Theme.accentStrong
                                    : Color.secondary.opacity(ThemeOpacity.tint)
                            )
                    )
            }
            .foregroundStyle(isActive ? Theme.accent : Color.secondary)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(isActive ? Theme.accentSoft : Color.clear)
            )
            .contentShape(Rectangle())
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
