import BaeKit
import SwiftUI

/// The first-run chooser: add this device to one of your own libraries with
/// its code, join someone's library by invitation, or restore from a code
/// coven saved in iCloud Keychain.
struct OnboardingEntryScreen: View {
    let setup: LibrarySetup
    let onJoin: () -> Void
    let onAddDevice: () -> Void
    let onRestoreSaved: (SavedRestoreCode) -> Void

    @Environment(LibraryRemovals.self)
    private var removals

    /// Restore codes coven saved in iCloud Keychain.
    @State
    private var saved = SavedRestoreCodes()

    private static let buttonWidth: CGFloat = 240

    var body: some View {
        ScrollView {
            OnboardingScreen {
                Image(systemName: "music.note.house.fill")
                    .themeIcon(.hero)
                    .foregroundStyle(Theme.accent)
                Text("bae")
                    .themeText(.wordmark)
                OnboardingSecondaryText(
                    "Add this device to a library you already have, or to one you were invited to."
                )

                VStack(spacing: ThemeSpace.group) {
                    Button(action: onAddDevice) {
                        Text("Add device")
                            .frame(maxWidth: Self.buttonWidth)
                    }
                    .buttonStyle(PrimaryButtonStyle())

                    Button(action: onJoin) {
                        Text("Join library")
                            .frame(maxWidth: Self.buttonWidth)
                    }
                    .buttonStyle(.bordered)
                }
                .padding(.top, ThemeSpace.edge)

                savedCodesSection
                LibraryRemovalNotice()
            }
        }
        .scrollBounceBehavior(.basedOnSize)
        .task { await saved.load(from: setup) }
    }

    @ViewBuilder
    private var savedCodesSection: some View {
        switch saved.list {
        case .loading:
            EmptyView()
        case .failed(let error):
            ErrorNotice(
                title: "Couldn't read recovery codes from your keychain",
                error: error
            )
        case .loaded(let entries) where !entries.isEmpty:
            SavedRestoreCodesList(
                codes: saved,
                setup: setup,
                disabled: removals.isRetrying,
                onRestore: onRestoreSaved
            )
        case .loaded:
            EmptyView()
        }
    }
}

/// A failed keychain read or delete: what failed, and the error.
private struct ErrorNotice: View {
    let title: LocalizedStringKey
    let error: DisplayError

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(title)
                .themeText(.strong)
            ErrorDetailDisclosure(error: error, showIcon: false)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notice(.danger)
    }
}

/// One row per restore code coven saved in iCloud Keychain, with Restore and
/// Delete. Each row shows the library's name as the person's devices last saw
/// it, over the provider and the account or bucket the code holds: two
/// libraries can share a name, or one renamed on another device still shows
/// its old name, and the location tells them apart. An entry bae can't read
/// shows why and its library id, with Delete only. Delete asks first, then
/// removes the code from iCloud Keychain on all the person's Apple devices.
private struct SavedRestoreCodesList: View {
    let codes: SavedRestoreCodes
    let setup: LibrarySetup
    let disabled: Bool
    let onRestore: (SavedRestoreCode) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text("Restore from iCloud Keychain")
                .themeText(.heading)
            Text(
                "Recovery codes saved by your other devices signed in to this Apple Account."
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            if let deleteError = codes.deleteError {
                ErrorNotice(
                    title: "Couldn't delete the recovery code",
                    error: deleteError
                )
            }
            ForEach(codes.list.value ?? [], id: \.libraryId) { entry in
                SavedCodeRow(
                    entry: entry,
                    isDeleting: codes.deletingLibraryId == entry.libraryId,
                    onRestore: onRestore,
                    onDelete: { codes.askToDelete(entry) }
                )
            }
        }
        .frame(maxWidth: 360)
        .padding(.top, ThemeSpace.section)
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
                // The person confirmed; the delete finishes even if this
                // screen closes meanwhile.
                Task { await codes.delete(pending, from: setup) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { pending in
            switch pending {
            case .readable(let saved):
                Text(
                    "This removes the recovery code for “\(saved.info.name)” from iCloud Keychain on all your Apple devices. It doesn't delete the library, or anything on this device or in your storage."
                )
            case .unreadable:
                Text(
                    "This removes the recovery code from iCloud Keychain on all your Apple devices. It doesn't delete the library, or anything on this device or in your storage."
                )
            }
        }
    }
}

/// One saved entry. A readable code shows the library's name, its provider
/// and location, and Restore; one bae can't read says why, with its library
/// id. Both have Delete, which shows progress while the delete runs.
private struct SavedCodeRow: View {
    let entry: SavedRestoreEntry
    let isDeleting: Bool
    let onRestore: (SavedRestoreCode) -> Void
    let onDelete: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            switch entry {
            case .readable(let saved):
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text(saved.info.name)
                        .themeText(.rowTitle)
                        .lineLimit(1)
                    Text(saved.info.location.accountLine)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer()
                Button("Restore") { onRestore(saved) }
                    .buttonStyle(.bordered)
            case .unreadable(let libraryId, let reason):
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text("Recovery code can't be used")
                        .themeText(.rowTitle)
                        .lineLimit(1)
                    Text(reason.label)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Text(verbatim: libraryId)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer()
            }
            ZStack {
                Button("Delete", systemImage: "trash", role: .destructive) {
                    onDelete()
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.bordered)
                .opacity(isDeleting ? 0 : 1)
                .allowsHitTesting(!isDeleting)
                ProgressView()
                    .opacity(isDeleting ? 1 : 0)
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
        .background(Color.secondary.opacity(ThemeOpacity.tint))
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
    }
}

#if DEBUG
#Preview {
    PreviewScenes.welcome()
}

#Preview("Saved codes") {
    OnboardingEntryScreen(
        setup: .stub(savedCodes: PreviewSavedRestoreCode.entries),
        onJoin: {},
        onAddDevice: {},
        onRestoreSaved: { _ in }
    )
    .background(Theme.background)
    .environment(PreviewData.removals())
}

#Preview("Delete confirmation") {
    let entries = PreviewSavedRestoreCode.entries.map(SavedRestoreEntry.init)
    SavedRestoreCodesList(
        codes: SavedRestoreCodes(previewing: entries, confirming: entries[0]),
        setup: .stub(savedCodes: PreviewSavedRestoreCode.entries),
        disabled: false,
        onRestore: { _ in }
    )
    .padding()
    .background(Theme.background)
}

#Preview("Unreadable code delete confirmation") {
    let entries = PreviewSavedRestoreCode.entries.map(SavedRestoreEntry.init)
    SavedRestoreCodesList(
        codes: SavedRestoreCodes(previewing: entries, confirming: entries.last),
        setup: .stub(savedCodes: PreviewSavedRestoreCode.entries),
        disabled: false,
        onRestore: { _ in }
    )
    .padding()
    .background(Theme.background)
}
#endif
