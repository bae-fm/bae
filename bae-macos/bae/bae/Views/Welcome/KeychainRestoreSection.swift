import BaeKit
import SwiftUI

/// One row per restore code coven saved in iCloud Keychain whose library isn't
/// on this Mac yet. A code carries no name, so each row shows the provider and
/// where the library is stored.
struct KeychainRestoreSection: View {
    let entries: [SavedRestoreEntry]
    let disabled: Bool
    let onRestore: (SavedRestoreEntry) -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            WelcomeSectionHeader(
                title: "Restore from iCloud Keychain",
                infoTip: InfoTip(
                    text:
                        "Restore codes saved by your other devices signed in to this Apple Account."
                )
            )
            ForEach(entries, id: \.info.libraryId) { entry in
                HStack(spacing: ThemeSpace.related) {
                    VStack(alignment: .leading, spacing: ThemeSpace.line) {
                        Text(entry.info.location.displayName)
                            .themeText(.rowTitle)
                        Text(entry.info.location.locationSummary)
                            .themeText(.detail)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer()
                    Button("Restore") { onRestore(entry) }
                        .buttonStyle(.bordered)
                }
                .padding(.horizontal, ThemeSpace.edge)
                .padding(.vertical, ThemeSpace.related)
                .background(Color.secondary.opacity(ThemeOpacity.tint))
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
        .disabled(disabled)
    }
}
