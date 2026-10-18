import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("OnboardingEntryScreen")

/// The first-run chooser: restore a library with your restore code, join one
/// by invitation, or restore from a code coven saved in iCloud Keychain.
struct OnboardingEntryScreen: View {
    let savedRestoreCodes: @Sendable () async throws -> [BridgeSavedRestoreCode]
    let onJoin: () -> Void
    let onRestore: () -> Void
    let onRestoreSaved: (SavedRestoreEntry) -> Void

    @Environment(LibraryRemovals.self)
    private var removals

    /// Restore codes coven saved in iCloud Keychain.
    @State
    private var saved: SavedCodes = .loading

    /// A refused lookup is kept apart from an empty one: a locked keychain
    /// doesn't mean there are no restore codes.
    private enum SavedCodes {
        case loading
        case loaded([SavedRestoreEntry])
        case failed(DisplayError)
    }

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
                    Button(action: onRestore) {
                        Text("Restore with your code")
                            .frame(maxWidth: Self.buttonWidth)
                    }
                    .buttonStyle(PrimaryButtonStyle())

                    Button(action: onJoin) {
                        Text("Join a library")
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
        .task { await loadSavedCodes() }
    }

    @ViewBuilder
    private var savedCodesSection: some View {
        switch saved {
        case .loading:
            EmptyView()
        case .failed(let error):
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text("Couldn't read restore codes from your keychain")
                    .themeText(.strong)
                ErrorDetailDisclosure(error: error, showIcon: false)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .notice(.danger)
        case .loaded(let entries) where !entries.isEmpty:
            SavedRestoreCodesList(
                entries: entries,
                disabled: removals.isRetrying,
                onRestore: onRestoreSaved
            )
        case .loaded:
            EmptyView()
        }
    }

    private func loadSavedCodes() async {
        do {
            let codes = try await savedRestoreCodes()
            try Task.checkCancellation()
            saved = .loaded(codes.map { SavedRestoreEntry(code: $0) })
        }
        catch is CancellationError {
            logger.debug("Saved restore code lookup cancelled")
        }
        catch {
            // A refused lookup (such as a locked keychain) isn't the same as
            // no restore codes. No line means core reported a cancellation.
            guard let failure = DisplayError(error) else {
                logger.debug("Saved restore code lookup cancelled")
                return
            }
            logger.error(
                "Saved restore code lookup failed: \(error.localizedDescription)"
            )
            saved = .failed(failure)
        }
    }
}

/// One row per restore code coven saved in iCloud Keychain. A code carries no
/// name, so each row shows the provider and the account or bucket the library
/// is in.
private struct SavedRestoreCodesList: View {
    let entries: [SavedRestoreEntry]
    let disabled: Bool
    let onRestore: (SavedRestoreEntry) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text("Restore from iCloud Keychain")
                .themeText(.heading)
            Text(
                "Restore codes saved by your other devices signed in to this Apple Account."
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            ForEach(entries, id: \.info.libraryId) { entry in
                HStack(spacing: ThemeSpace.related) {
                    Text(entry.info.location.accountLine)
                        .themeText(.rowTitle)
                        .lineLimit(1)
                        .truncationMode(.middle)
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
        .frame(maxWidth: 360)
        .padding(.top, ThemeSpace.section)
        .disabled(disabled)
    }
}

#if DEBUG
#Preview {
    PreviewScenes.welcome()
}
#endif
