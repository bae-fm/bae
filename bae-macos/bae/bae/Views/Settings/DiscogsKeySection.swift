import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("DiscogsKey")

/// The Discogs key row under the Discogs source switch in Import settings: the
/// stored key's state and the actions that change it.
struct DiscogsKeySection: View {
    @Environment(Discogs.self)
    var discogs
    @Environment(ConfigStore.self)
    var configStore

    /// The key being typed, seeded from the keyring and kept after a rejected
    /// save so a typo can be fixed.
    @State
    private var draft: String = ""
    /// The in-flight save or re-check.
    @State
    private var saveTask: Task<Void, Never>?
    /// The last save, re-check, or remove error, including Discogs rejecting
    /// the typed key.
    @State
    private var saveError: String?
    /// Set when the stored key can't be read back for display.
    @State
    private var readError: String?

    var body: some View {
        DiscogsSettingsContent(
            draft: $draft,
            status: configStore.config.discogsTokenStatus,
            isValidating: saveTask != nil,
            saveError: saveError,
            readError: readError,
            onSave: { saveToken() },
            onRecheck: { revalidate() },
            onRemove: { removeToken() },
        )
        .task {
            await seedDraftFromStoredKey()
            // Core does nothing unless the stored key is unvalidated.
            revalidate()
        }
        .onDisappear { saveTask?.cancel() }
    }

    /// Fills the draft with the stored key when one is configured.
    private func seedDraftFromStoredKey() async {
        guard configStore.config.discogsTokenStatus != .notConfigured else {
            return
        }
        do {
            if let stored = try await discogs.getDiscogsToken() {
                draft = stored
            }
            else {
                logger.warning(
                    "Discogs status says a key is configured but the keyring returned none"
                )
            }
        }
        catch {
            // A nil line means a cancellation, which shows nothing.
            readError = error.displayLine.map { line in
                String(
                    localized: "Couldn't read the stored Discogs key: \(line)"
                )
            }
        }
    }

    private func saveToken() {
        saveTask?.cancel()
        saveError = nil
        readError = nil
        let token = draft
        let discogs = discogs
        saveTask = Task {
            defer { saveTask = nil }
            do {
                let outcome = try await discogs.saveDiscogsToken(token)
                if case .rejected = outcome {
                    saveError = String(
                        localized:
                            "Discogs rejected this key. Check it and save again."
                    )
                }
                // Other outcomes reach the status through the config event.
            }
            catch is CancellationError {
                logger.debug("saveToken cancelled")
            }
            catch {
                saveError = error.displayLine.map { line in
                    String(localized: "Couldn't save the Discogs key: \(line)")
                }
            }
        }
    }

    private func revalidate() {
        saveTask?.cancel()
        let discogs = discogs
        saveTask = Task {
            defer { saveTask = nil }
            do {
                try await discogs.revalidateDiscogsToken()
            }
            catch is CancellationError {
                logger.debug("revalidate cancelled")
            }
            catch {
                saveError = error.displayLine.map { line in
                    String(
                        localized: "Couldn't re-check the Discogs key: \(line)"
                    )
                }
            }
        }
    }

    private func removeToken() {
        saveError = nil
        readError = nil
        Task {
            do {
                try await discogs.removeDiscogsToken()
                draft = ""
            }
            catch {
                saveError = error.displayLine.map { line in
                    String(
                        localized: "Couldn't remove the Discogs key: \(line)"
                    )
                }
            }
        }
    }
}

// MARK: - DiscogsSettingsContent (pure leaf)

/// One Form row: the state of the stored key with the buttons that state
/// offers, any error from the last action, and what the key is for.
struct DiscogsSettingsContent: View {
    @Binding
    var draft: String
    let status: BridgeDiscogsTokenStatus
    let isValidating: Bool
    let saveError: String?
    let readError: String?
    let onSave: () -> Void
    let onRecheck: () -> Void
    let onRemove: () -> Void
    @FocusState
    private var keyFieldIsFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            statusRow
            if let saveError {
                ErrorText(saveError)
            }
            if let readError {
                ErrorText(readError)
            }
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text(
                    "[Discogs](https://www.discogs.com) is a music database with detailed release info: labels, catalog numbers, pressing variants, and more. bae can use it as a metadata source when importing albums."
                )
                Text(
                    "To connect, [get your free API key](https://www.discogs.com/settings/developers) and paste it above."
                )
            }
            .themeText(.body)
            .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private var statusRow: some View {
        switch status {
        case .notConfigured, .rejected:
            keyInput
        case .valid:
            connectedRow
        case .unvalidated:
            unvalidatedRow
        }
    }

    /// Key field and Save, for the not-configured and rejected states.
    private var keyInput: some View {
        HStack(spacing: ThemeSpace.compact) {
            TextField(
                "API key",
                text: $draft,
                prompt: Text("Paste your key here")
            )
            .labelsHidden()
            .focused($keyFieldIsFocused)
            .task {
                await Task.yield()
                keyFieldIsFocused = true
            }
            if isValidating {
                ProgressView().controlSize(.small)
            }
            Button("Save", action: onSave)
                .disabled(draft.isEmpty || isValidating)
        }
    }

    private var connectedRow: some View {
        HStack(spacing: ThemeSpace.compact) {
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(Theme.success)
            Text("Connected")
                .foregroundStyle(.secondary)
            Spacer()
            Button("Remove", action: onRemove)
        }
    }

    private var unvalidatedRow: some View {
        HStack(spacing: ThemeSpace.compact) {
            if isValidating {
                ProgressView().controlSize(.small)
            }
            else {
                Image(systemName: "exclamationmark.circle.fill")
                    .foregroundStyle(Theme.warning)
            }
            Text("Saved. Couldn't validate yet (offline). Will retry.")
                .foregroundStyle(.secondary)
            Spacer()
            Button("Re-check", action: onRecheck)
                .disabled(isValidating)
            Button("Remove", action: onRemove)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// The key as the Sources section draws it: one row of a grouped Form.
    private struct DiscogsKeyPreview: View {
        let status: BridgeDiscogsTokenStatus
        var draft: String = ""
        var saveError: String?

        var body: some View {
            Form {
                Section {
                    DiscogsSettingsContent(
                        draft: .constant(draft),
                        status: status,
                        isValidating: false,
                        saveError: saveError,
                        readError: nil,
                        onSave: {},
                        onRecheck: {},
                        onRemove: {},
                    )
                }
            }
            .formStyle(.grouped)
            .frame(width: 500, height: 320)
        }
    }

    #Preview("No key") {
        DiscogsKeyPreview(status: .notConfigured)
    }

    #Preview("Valid") {
        DiscogsKeyPreview(status: .valid, draft: "abcdef123456")
    }

    #Preview("Unvalidated") {
        DiscogsKeyPreview(status: .unvalidated, draft: "abcdef123456")
    }

    #Preview("Rejected") {
        DiscogsKeyPreview(
            status: .rejected,
            draft: "bad-key",
            saveError: "Discogs rejected this key. Check it and save again."
        )
    }
#endif
