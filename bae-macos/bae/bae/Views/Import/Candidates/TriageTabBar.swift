import BaeKit
import SwiftUI

/// The Found / Imported / Skipped tab bar above the candidate list, with
/// every tab's count from the list's count, which a filter does not change.
struct TriageTabBar: View {
    @Binding
    var activeTab: BridgeTriageTab
    /// `nil` until the list's count arrives; each chip is empty until then.
    let counts: BridgeTriageTabCounts?

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            segment(.pending)
            segment(.done)
            segment(.skipped)
        }
    }

    private func segment(_ tab: BridgeTriageTab) -> some View {
        let isActive = activeTab == tab
        return Button {
            activeTab = tab
        } label: {
            HStack(spacing: ThemeSpace.inline) {
                Text(verbatim: MacImportFolderLine.label(for: tab))
                    .themeText(.strong)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                let tone: StatusTone = isActive ? .accent : .neutral
                Text(
                    verbatim: counts.map { tab.count(in: $0).formatted() } ?? ""
                )
                .themeText(.chip)
                .monospacedDigit()
                .foregroundStyle(tone.color)
                .lineLimit(1)
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .background(
                    tone.fill,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
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
        .buttonStyle(.accentPlain)
    }
}

extension BridgeTriageTab {
    /// The tab's count among `counts`.
    func count(in counts: BridgeTriageTabCounts) -> UInt32 {
        switch self {
        case .pending: counts.pending
        case .done: counts.done
        case .skipped: counts.skipped
        }
    }
}
