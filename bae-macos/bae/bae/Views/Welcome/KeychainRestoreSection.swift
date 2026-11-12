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
        .macAlert(
            item: Binding(
                get: { codes.pendingDelete },
                set: { if $0 == nil { codes.dismissDelete() } }
            )
        ) { pending in
            SavedCodeDeletion.alert {
                // The person confirmed; the delete finishes even if the
                // welcome closes meanwhile.
                Task { await codes.delete(pending, from: setup) }
            }
        }
    }
}

/// What the confirmation to delete a saved restore code says.
enum SavedCodeDeletion {
    /// The alert; Delete runs `onDelete`.
    static func alert(
        onDelete: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text("Delete this restore code?"),
                message: Text(verbatim: message)
            ),
            [
                MacAlertButton(
                    Text("Delete"),
                    role: .destructive,
                    action: onDelete
                ),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }

    /// That the code goes from iCloud Keychain on all the person's Apple
    /// devices and nothing else does.
    static var message: String {
        String(
            localized:
                "This removes the restore code from iCloud Keychain on all your Apple devices. It does not delete the library, or anything on this Mac or in the cloud."
        )
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
                    .foregroundStyle(Theme.textPrimary)
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
            String(localized: "Restore code cannot be used")
        }
    }

    private var detail: String {
        switch entry {
        case .readable(let saved):
            // The provider and the account side by side, as the unreadable
            // line puts its reason and library.
            saved.info.location.accountLabel.map {
                "\(saved.info.location.displayName) · \($0)"
            } ?? saved.info.location.accountLine
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
