import BaeKit
import SwiftUI

/// The Found / Imported / Skipped tab bar above the candidate list, with
/// counts from core's tab counts stream, which a filter does not change. A
/// count moves with every commit of a scan, so each is a label that reads its
/// tab's latest value each display frame, not text SwiftUI redraws.
struct TriageTabBar: View {
    @Binding
    var activeTab: BridgeTriageTab
    let counts: LatestStream<UInt32>

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
                Text(verbatim: CandidateFolderLine.label(for: tab))
                    .themeText(.strong)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                let tone: StatusTone = isActive ? .accent : .neutral
                StreamLabel(
                    stream: .tabCount(
                        counts.source(for: tab.countKey),
                        tone: tone
                    )
                )
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
    /// The key the tab's count is under in the tab counts stream.
    var countKey: Data {
        switch self {
        case .pending: Data("pending".utf8)
        case .done: Data("done".utf8)
        case .skipped: Data("skipped".utf8)
        }
    }
}

#if DEBUG
    // MARK: - Previews

#endif
