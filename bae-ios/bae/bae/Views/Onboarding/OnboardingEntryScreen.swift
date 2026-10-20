import BaeKit
import SwiftUI

/// The first-run chooser: add this device to one of your own libraries with
/// its code, join someone's library by invitation, or restore from a code
/// coven saved in iCloud Keychain.
struct OnboardingEntryScreen: View {
    let setup: LibrarySetup
    let onJoin: () -> Void
    let onAddDevice: () -> Void
    let onRestoreSaved: (SavedRestoreEntry) -> Void

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
/// its old name, and the location tells them apart. Delete asks first, then
/// removes the code from iCloud Keychain on all the person's Apple devices.
private struct SavedRestoreCodesList: View {
    let codes: SavedRestoreCodes
    let setup: LibrarySetup
    let disabled: Bool
    let onRestore: (SavedRestoreEntry) -> Void

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
            ForEach(codes.list.value ?? [], id: \.info.libraryId) { entry in
                SavedCodeRow(
                    entry: entry,
                    isDeleting: codes.deletingLibraryId == entry.info.libraryId,
                    onRestore: onRestore,
                    onDelete: { codes.askToDelete($0) }
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
            Text(
                "This removes the recovery code for “\(pending.info.name)” from iCloud Keychain on all your Apple devices. It doesn't delete the library, or anything on this device or in your storage."
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
        HStack(spacing: ThemeSpace.related) {
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
            Spacer()
            Button("Restore") { onRestore(entry) }
                .buttonStyle(.bordered)
            ZStack {
                Button("Delete", systemImage: "trash", role: .destructive) {
                    onDelete(entry)
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
        setup: .stub(savedCodes: PreviewSavedRestoreCode.samples),
        onJoin: {},
        onAddDevice: {},
        onRestoreSaved: { _ in }
    )
    .background(Theme.background)
    .environment(PreviewData.removals())
}

#Preview("Delete confirmation") {
    let entries = PreviewSavedRestoreCode.samples.map {
        SavedRestoreEntry(code: PreviewSavedRestoreCode($0))
    }
    SavedRestoreCodesList(
        codes: SavedRestoreCodes(previewing: entries, confirming: entries[0]),
        setup: .stub(savedCodes: PreviewSavedRestoreCode.samples),
        disabled: false,
        onRestore: { _ in }
    )
    .padding()
    .background(Theme.background)
}
#endif
