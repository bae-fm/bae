import SwiftUI

/// The small uppercase caption above a welcome-screen section, optionally
/// trailed by an info tip. The libraries and keychain sections both use it so
/// their headers read as one style and align to the same leading edge as the
/// column below them.
struct WelcomeSectionHeader: View {
    let title: LocalizedStringKey
    var infoTip: InfoTip?

    var body: some View {
        HStack(spacing: 4) {
            Text(title)
                .font(.caption)
                .fontWeight(.semibold)
                .textCase(.uppercase)
                .foregroundStyle(.secondary)
            // Optional<InfoTip> renders nothing when nil; this header appears
            // once per section (not in a ForEach), so the conditional is fine.
            infoTip
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
