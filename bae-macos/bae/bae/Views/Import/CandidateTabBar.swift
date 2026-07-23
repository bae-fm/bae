import SwiftUI

/// The New / Added / Skipped tab bar above the candidate list. Three equal
/// segments, each a label plus a count badge; the active one is highlighted.
struct CandidateTabBar: View {
    @Binding
    var activeTab: CandidateTab
    /// Read the counts at the leaf so the parent isn't subscribed to every
    /// candidate change just to pass them down.
    let importStore: ImportStore

    var body: some View {
        let counts = importStore.candidateTabCounts()
        HStack(spacing: 4) {
            segment(.new, "New", counts.new)
            segment(.added, "Added", counts.added)
            segment(.skipped, "Skipped", counts.skipped)
        }
    }

    private func segment(
        _ tab: CandidateTab,
        _ label: LocalizedStringKey,
        _ count: Int
    ) -> some View {
        let isActive = activeTab == tab
        return Button {
            activeTab = tab
        } label: {
            HStack(spacing: 4) {
                Text(label)
                    .font(.caption)
                    .fontWeight(isActive ? .semibold : .regular)
                Text(verbatim: count.formatted())
                    .font(.caption2)
                    .monospacedDigit()
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(
                        Capsule()
                            .fill(
                                isActive
                                    ? Color.accentColor.opacity(0.25)
                                    : Color.secondary.opacity(0.15)
                            )
                    )
            }
            .foregroundStyle(isActive ? Color.accentColor : Color.secondary)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(
                        isActive
                            ? Color.accentColor.opacity(0.12) : Color.clear
                    )
            )
        }
        .buttonStyle(.plain)
    }
}
