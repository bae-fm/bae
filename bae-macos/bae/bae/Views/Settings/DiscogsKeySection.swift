import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("DiscogsKey")

/// The Discogs key, drawn under the Discogs source switch in the Import
/// settings: the stored key's state and the actions that change it. One Form
/// row, so the lifecycle below runs once however many controls the state draws.
struct DiscogsKeySection: View {
    @Environment(Discogs.self)
    var discogs
    @Environment(ConfigStore.self)
    var configStore

    /// The editable key the user types. A draft, distinct from the stored key:
    /// seeded once from the keyring when a key is configured (so the user sees
    /// what's stored), kept across a rejected save so a typo is correctable.
    @State
    private var draft: String = ""
    /// The in-flight save/revalidate task. Event-driven (button tap), so it's
    /// held in `@State` and cancelled from handlers — not `.task(id:)`.
    @State
    private var saveTask: Task<Void, Never>?
    /// Set only when Discogs rejected the typed key (401). Cleared on a save
    /// that stores the key. Distinct from the persisted status.
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
            seedDraftFromStoredKey()
            // Core no-ops unless the stored key is `Unvalidated`, so call
            // unconditionally rather than inspecting the status here.
            revalidate()
        }
        .onDisappear { saveTask?.cancel() }
    }

    /// Form-state seeding: render the stored key into the editable draft when a
    /// key is configured. The empty string is the input's "no value".
    private func seedDraftFromStoredKey() {
        guard configStore.config.discogsTokenStatus != .notConfigured else {
            return
        }
        do {
            if let stored = try discogs.getDiscogsToken() {
                draft = stored
            }
            else {
                // Status says a key is configured but the keyring has none —
                // the config flag and the keyring disagree.
                logger.warning(
                    "Discogs status says a key is configured but the keyring returned none"
                )
            }
        }
        catch {
            // Nil line means core reported a cancellation, which has nothing to
            // say; the field stays clear rather than showing `Optional("…")`.
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
                // `.valid` / `.unvalidated` update the persisted status
                // reactively through the config event; nothing to set here.
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
        do {
            try discogs.removeDiscogsToken()
            draft = ""
        }
        catch {
            saveError = error.displayLine.map { line in
                String(localized: "Couldn't remove the Discogs key: \(line)")
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
        VStack(alignment: .leading, spacing: 8) {
            statusRow
            if let saveError {
                Text(saveError)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
            if let readError {
                Text(readError)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
            VStack(alignment: .leading, spacing: 8) {
                Text(
                    "[Discogs](https://www.discogs.com) is a music database with detailed release info: labels, catalog numbers, pressing variants, and more. bae can use it as a metadata source when importing albums."
                )
                Text(
                    "To connect, [get your free API key](https://www.discogs.com/settings/developers) and paste it above."
                )
            }
            .font(.callout)
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

    /// Editable field + Save, for the not-configured and rejected states. In
    /// `rejected` the draft is kept so the user corrects a typo.
    private var keyInput: some View {
        HStack(spacing: 6) {
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
        HStack(spacing: 6) {
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green)
            Text("Connected")
                .foregroundStyle(.secondary)
            Spacer()
            Button("Remove", action: onRemove)
        }
    }

    private var unvalidatedRow: some View {
        HStack(spacing: 6) {
            if isValidating {
                ProgressView().controlSize(.small)
            }
            else {
                Image(systemName: "exclamationmark.circle.fill")
                    .foregroundStyle(.orange)
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
