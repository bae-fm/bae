import BaeKit
import SwiftUI

/// One row per restore code coven saved in iCloud Keychain whose library isn't
/// on this Mac yet, with Restore and Delete. Each row shows the library's name
/// as the person's devices last saw it, over the provider and the account or
/// bucket the code holds: two libraries can share a name, or one renamed on
/// another device still shows its old name, and the location tells them apart.
/// Delete asks first, then removes the code from iCloud Keychain on all the
/// person's Apple devices.
struct KeychainRestoreSection: View {
    let codes: SavedRestoreCodes
    let disabled: Bool
    let onRestore: (SavedRestoreEntry) -> Void

    @Environment(LibrarySetup.self)
    private var setup

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            WelcomeSectionHeader(
                title: "Restore from iCloud Keychain",
                infoTip: InfoTip(
                    text:
                        "Recovery codes saved by your other devices signed in to this Apple Account."
                )
            )
            ForEach(codes.list.value ?? [], id: \.info.libraryId) { entry in
                SavedCodeRow(
                    entry: entry,
                    isDeleting: codes.deletingLibraryId == entry.info.libraryId,
                    onRestore: onRestore,
                    onDelete: { codes.askToDelete($0) }
                )
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
        .disabled(disabled || codes.deletingLibraryId != nil)
        .alert(
            "Delete this recovery code?",
            isPresented: Binding(
                get: { codes.pendingDelete != nil },
                set: { if !$0 { codes.dismissDelete() } }
            ),
            presenting: codes.pendingDelete
        ) { pending in
            Button("Delete", role: .destructive) {
                // The person confirmed; the delete finishes even if the
                // welcome closes meanwhile.
                Task { await codes.delete(pending, from: setup) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { pending in
            Text(
                "This removes the recovery code for “\(pending.info.name)” from iCloud Keychain on all your Apple devices. It doesn't delete the library, or anything on this Mac or in your storage."
            )
        }
    }
}

/// One saved code: the library's name, its provider and location, Restore,
/// and Delete, which shows progress while the delete runs.
private struct SavedCodeRow: View {
    let entry: SavedRestoreEntry
    let isDeleting: Bool
    let onRestore: (SavedRestoreEntry) -> Void
    let onDelete: (SavedRestoreEntry) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(entry.info.name)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Text(entry.info.location.accountLine)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: ThemeSpace.group)
            Button("Restore") { onRestore(entry) }
                .buttonStyle(.bordered)
            ZStack {
                Button("Delete", systemImage: "trash", role: .destructive) {
                    onDelete(entry)
                }
                .buttonStyle(.bordered)
                .opacity(isDeleting ? 0 : 1)
                .allowsHitTesting(!isDeleting)
                ProgressView()
                    .controlSize(.small)
                    .opacity(isDeleting ? 1 : 0)
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.secondary.opacity(ThemeOpacity.tint))
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
    }
}

#if DEBUG
    #Preview("Saved codes") {
        KeychainRestoreSection(
            codes: SavedRestoreCodes(
                previewing: PreviewData.welcomeSavedCodes
            ),
            disabled: false,
            onRestore: { _ in }
        )
        .padding()
        .environment(PreviewData.welcomeSetup())
    }

    #Preview("Delete confirmation") {
        let entries = PreviewData.welcomeSavedCodes
        KeychainRestoreSection(
            codes: SavedRestoreCodes(
                previewing: entries,
                confirming: entries[0]
            ),
            disabled: false,
            onRestore: { _ in }
        )
        .padding()
        .environment(PreviewData.welcomeSetup())
    }
#endif
