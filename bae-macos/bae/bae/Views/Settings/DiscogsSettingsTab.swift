import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("DiscogsSettings")

struct DiscogsSettingsTab: View {
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
            readError = String(
                localized:
                    "Couldn't read the stored Discogs key: \(error.localizedDescription)"
            )
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
                            "Discogs rejected this key — check it and save again."
                    )
                }
                // `.valid` / `.unvalidated` update the persisted status
                // reactively through the config event; nothing to set here.
            }
            catch is CancellationError {
                logger.debug("saveToken cancelled")
            }
            catch {
                saveError = String(
                    localized:
                        "Couldn't save the Discogs key: \(error.localizedDescription)"
                )
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
                saveError = String(
                    localized:
                        "Couldn't re-check the Discogs key: \(error.localizedDescription)"
                )
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
            saveError = String(
                localized:
                    "Couldn't remove the Discogs key: \(error.localizedDescription)"
            )
        }
    }
}

// MARK: - DiscogsSettingsContent (pure leaf)

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

    var body: some View {
        Form {
            Section {
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
            }

            Section {
                VStack(alignment: .leading, spacing: 8) {
                    Text(
                        "[Discogs](https://www.discogs.com) is a music database with detailed release info — labels, catalog numbers, pressing variants, and more. bae can use it as a metadata source when importing albums."
                    )
                    Text(
                        "To connect, [get your free API key](https://www.discogs.com/settings/developers) and paste it above."
                    )
                }
                .font(.callout)
                .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }

    @ViewBuilder
    private var statusRow: some View {
        switch status {
        case .notConfigured, .rejected:
            keyInput
        case .valid:
            connectedRow(
                label: "Connected",
                systemImage: "checkmark.circle.fill",
                tint: .green
            )
        case .unvalidated:
            unvalidatedRow
        }
    }

    /// Editable field + Save, for the not-configured and rejected states. In
    /// `rejected` the draft is kept so the user corrects a typo.
    @ViewBuilder
    private var keyInput: some View {
        HStack(spacing: 6) {
            TextField(
                "API key",
                text: $draft,
                prompt: Text("Paste your key here")
            )
            if isValidating {
                ProgressView().controlSize(.small)
            }
        }
        HStack {
            Spacer()
            Button("Save", action: onSave)
                .disabled(draft.isEmpty || isValidating)
        }
    }

    @ViewBuilder
    private func connectedRow(
        label: LocalizedStringKey,
        systemImage: String,
        tint: Color
    ) -> some View {
        LabeledContent("Discogs") {
            HStack(spacing: 6) {
                Image(systemName: systemImage).foregroundStyle(tint)
                Text(label).foregroundStyle(.secondary)
            }
        }
        HStack {
            Spacer()
            Button("Remove", action: onRemove)
        }
    }

    @ViewBuilder
    private var unvalidatedRow: some View {
        LabeledContent("Discogs") {
            HStack(spacing: 6) {
                if isValidating {
                    ProgressView().controlSize(.small)
                }
                else {
                    Image(systemName: "exclamationmark.circle.fill")
                        .foregroundStyle(.orange)
                }
                Text("Saved — couldn't validate yet (offline). Will retry.")
                    .foregroundStyle(.secondary)
            }
        }
        HStack {
            Spacer()
            Button("Re-check", action: onRecheck)
                .disabled(isValidating)
            Button("Remove", action: onRemove)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("No key") {
        DiscogsSettingsContent(
            draft: .constant(""),
            status: .notConfigured,
            isValidating: false,
            saveError: nil,
            readError: nil,
            onSave: {},
            onRecheck: {},
            onRemove: {},
        )
        .frame(width: 500, height: 320)
    }

    #Preview("Valid") {
        DiscogsSettingsContent(
            draft: .constant("abcdef123456"),
            status: .valid,
            isValidating: false,
            saveError: nil,
            readError: nil,
            onSave: {},
            onRecheck: {},
            onRemove: {},
        )
        .frame(width: 500, height: 320)
    }

    #Preview("Unvalidated") {
        DiscogsSettingsContent(
            draft: .constant("abcdef123456"),
            status: .unvalidated,
            isValidating: false,
            saveError: nil,
            readError: nil,
            onSave: {},
            onRecheck: {},
            onRemove: {},
        )
        .frame(width: 500, height: 320)
    }

    #Preview("Rejected") {
        DiscogsSettingsContent(
            draft: .constant("bad-key"),
            status: .rejected,
            isValidating: false,
            saveError: "Discogs rejected this key — check it and save again.",
            readError: nil,
            onSave: {},
            onRecheck: {},
            onRemove: {},
        )
        .frame(width: 500, height: 320)
    }
#endif
