import BaeKit
import SwiftUI

/// The restore codes coven saved in iCloud Keychain for libraries not on this
/// Mac yet, in a box of rows under "Saved in iCloud Keychain", with a note under
/// the box. Each row shows the library's name over its provider and the account
/// or bucket the code holds: two libraries can share a name, or one renamed on
/// another device still shows its old name, and the location tells them apart.
/// A readable code has Restore; an entry bae can't read shows why and its
/// library id. Every row ends in a ⋯ menu holding Delete….
struct KeychainRestoreSection: View {
    let entries: [SavedRestoreEntry]
    let disabled: Bool
    let onRestore: (SavedRestoreCode) -> Void
    let onDelete: (SavedRestoreEntry) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            Text("Saved in iCloud Keychain")
                .themeText(.strong)
                .padding(.horizontal, ThemeSpace.inline)
            VStack(spacing: 0) {
                ForEach(
                    Array(entries.enumerated()),
                    id: \.element.libraryId
                ) { index, entry in
                    if index > 0 {
                        Divider()
                            .padding(.horizontal, ThemeSpace.group)
                            .opacity(0.6)
                    }
                    SavedCodeRow(
                        entry: entry,
                        onRestore: onRestore,
                        onDelete: { onDelete(entry) }
                    )
                }
            }
            .card()
            Text(
                "Restore codes your other Apple devices saved. Restoring brings that library onto this Mac."
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            .padding(.horizontal, ThemeSpace.inline)
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
        .disabled(disabled)
    }
}

/// The saved codes the welcome lists, read from `codes`: restoring one, and
/// deleting one, which asks first, then removes the code from iCloud Keychain
/// on all the person's Apple devices.
struct KeychainRestoreContainer: View {
    let codes: SavedRestoreCodes
    let disabled: Bool
    let onRestore: (SavedRestoreCode) -> Void

    @Environment(LibrarySetup.self)
    private var setup

    var body: some View {
        KeychainRestoreSection(
            entries: codes.list.value ?? [],
            disabled: disabled || codes.deletingLibraryId != nil,
            onRestore: onRestore,
            onDelete: { codes.askToDelete($0) }
        )
        .alert(
            SavedCodeDeletion.title,
            isPresented: Binding(
                get: { codes.pendingDelete != nil },
                set: { if !$0 { codes.dismissDelete() } }
            ),
            presenting: codes.pendingDelete
        ) { pending in
            Button(SavedCodeDeletion.confirm, role: .destructive) {
                // The person confirmed; the delete finishes even if the
                // welcome closes meanwhile.
                Task { await codes.delete(pending, from: setup) }
            }
            Button(SavedCodeDeletion.cancel, role: .cancel) {}
        } message: { pending in
            Text(verbatim: SavedCodeDeletion.message(for: pending))
        }
    }
}

/// What the confirmation to delete a saved restore code says.
enum SavedCodeDeletion {
    static var title: String { String(localized: "Delete this restore code?") }
    static var confirm: String { String(localized: "Delete") }
    static var cancel: String { String(localized: "Cancel") }

    /// That the code goes from iCloud Keychain on all the person's Apple
    /// devices and nothing else does.
    static func message(for entry: SavedRestoreEntry) -> String {
        switch entry {
        case .readable(let saved):
            String(
                localized:
                    "This removes the restore code for “\(saved.info.name)” from iCloud Keychain on all your Apple devices. It doesn’t delete the library, or anything on this Mac or in the cloud."
            )
        case .unreadable:
            String(
                localized:
                    "This removes the restore code from iCloud Keychain on all your Apple devices. It doesn’t delete the library, or anything on this Mac or in the cloud."
            )
        }
    }
}

/// One saved entry. A readable code shows the library's name over its provider
/// and location, with Restore; one bae can't read says why, with its library
/// id, and only deletes. Both end in the ⋯ menu.
private struct SavedCodeRow: View {
    let entry: SavedRestoreEntry
    let onRestore: (SavedRestoreCode) -> Void
    let onDelete: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(verbatim: title)
                    .themeText(.strong)
                    .lineLimit(1)
                Text(verbatim: detail)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: ThemeSpace.group)
            if case .readable(let saved) = entry {
                Button("Restore") { onRestore(saved) }
                    .buttonStyle(.bordered)
            }
            MacMoreMenu {
                Button("Delete…", action: onDelete)
            }
        }
        .padding(.horizontal, ThemeSpace.group)
        .padding(.vertical, ThemeSpace.related + ThemeSpace.line)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var title: String {
        switch entry {
        case .readable(let saved): saved.info.name
        case .unreadable:
            String(localized: "Restore code can’t be used")
        }
    }

    private var detail: String {
        switch entry {
        case .readable(let saved): saved.info.location.accountLine
        case .unreadable(let libraryId, let reason):
            "\(reason.label) · \(libraryId)"
        }
    }
}

#if DEBUG
    #Preview("Saved codes") {
        KeychainRestoreContainer(
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
        KeychainRestoreContainer(
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

    #Preview("Unreadable code delete confirmation") {
        let entries = PreviewData.welcomeSavedCodes
        KeychainRestoreContainer(
            codes: SavedRestoreCodes(
                previewing: entries,
                confirming: entries.last
            ),
            disabled: false,
            onRestore: { _ in }
        )
        .padding()
        .environment(PreviewData.welcomeSetup())
    }
#endif
