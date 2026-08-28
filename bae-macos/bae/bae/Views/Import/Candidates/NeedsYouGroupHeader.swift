import BaeKit
import SwiftUI

/// A Needs-you section header: the group's stable-key title, localized here,
/// and its row count. Non-collapsible — the six groups are the sidebar's own
/// structure, not a per-folder list a person might want to hide.
struct NeedsYouGroupHeader: View {
    let group: BridgeNeedsYouGroup
    let count: Int

    var body: some View {
        HStack(spacing: 6) {
            Text(title)
                .font(
                    .system(size: 10.5, weight: .semibold, design: .monospaced)
                )
                .tracking(1.3)
                .textCase(.uppercase)
                .foregroundStyle(.tertiary)
            Spacer()
            Text(verbatim: count.formatted())
                .font(.system(size: 10.5, design: .monospaced))
                .foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 14)
        .padding(.top, 14)
        .padding(.bottom, 7)
    }

    private var title: LocalizedStringKey {
        switch group {
        case .pickAPressing: "Pick a Pressing"
        case .countsOrLengthsDisagree: "Counts or Lengths Disagree"
        case .alreadyInLibrary: "Already in Library"
        case .noMatch: "No Match"
        case .stillIdentifying: "Still Identifying"
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Needs-you group headers") {
        VStack(alignment: .leading, spacing: 12) {
            NeedsYouGroupHeader(group: .pickAPressing, count: 31)
            NeedsYouGroupHeader(group: .countsOrLengthsDisagree, count: 10)
            NeedsYouGroupHeader(group: .alreadyInLibrary, count: 15)
            NeedsYouGroupHeader(group: .noMatch, count: 48)
            NeedsYouGroupHeader(group: .stillIdentifying, count: 18)
        }
        .padding()
        .frame(width: 320)
        .windowBackground()
    }
#endif
