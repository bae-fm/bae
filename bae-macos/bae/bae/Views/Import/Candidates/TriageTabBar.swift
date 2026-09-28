import BaeKit
import SwiftUI

/// The Found / Imported / Skipped tab bar above the candidate list, with
/// counts from core's `BridgeTriageTabCounts` so a filter does not change them.
struct TriageTabBar: View {
    @Binding
    var activeTab: BridgeTriageTab
    let counts: BridgeTriageTabCounts

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
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
            HStack(spacing: ThemeSpace.inline) {
                Text(verbatim: CandidateFolderLine.label(for: tab))
                    .themeText(.strong)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                StatusChip(
                    Text(verbatim: count.formatted()).monospacedDigit(),
                    tone: isActive ? .accent : .neutral
                )
                // A squeezed badge must overflow, never stack its digits.
                .fixedSize()
            }
            .foregroundStyle(isActive ? Theme.accent : Color.secondary)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, ThemeSpace.related)
            .padding(.vertical, ThemeSpace.compact)
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
