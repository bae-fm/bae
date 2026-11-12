import AppKit
import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("DiscogsKey")

/// The Discogs key under the Search Discogs switch in Import settings: the
/// field that takes it, and what Discogs said of the key saved.
struct DiscogsKeySection: View {
    @Environment(Discogs.self)
    var discogs
    @Environment(ConfigStore.self)
    var configStore

    /// Whether Search Discogs is switched on; off, the field is greyed.
    let isEnabled: Bool

    /// The key being typed, seeded from the keyring and kept after a rejected
    /// save so a typo can be fixed.
    @State
    private var draft: String = ""
    /// The in-flight save or re-check.
    @State
    private var saveTask: Task<Void, Never>?
    /// Whether Discogs turned down the key last saved. A rejected key is not
    /// stored, so core's status does not say so.
    @State
    private var wasRejected = false
    /// The last save or re-check error.
    @State
    private var saveError: DisplayError?
    /// Set when the stored key can't be read back for display.
    @State
    private var readError: DisplayError?

    var body: some View {
        let content = DiscogsSettingsContent(
            draft: $draft,
            status: configStore.config.discogsTokenStatus,
            isEnabled: isEnabled,
            isValidating: saveTask != nil,
            wasRejected: wasRejected,
            saveError: saveError,
            readError: readError,
            onSave: { saveToken() },
            onRecheck: { revalidate() },
        )
        Group {
            if !content.isBlank {
                content
            }
        }
        .task {
            await seedDraftFromStoredKey()
            // Core does nothing unless the stored key is unvalidated.
            revalidate()
        }
        .onChange(of: draft) { wasRejected = false }
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
            readError = DisplayError(error)?
                .rewording { line in
                    String(
                        localized:
                            "Could not read the stored Discogs key: \(line)"
                    )
                }
        }
    }

    private func saveToken() {
        saveTask?.cancel()
        saveError = nil
        readError = nil
        wasRejected = false
        let token = draft
        let discogs = discogs
        saveTask = Task {
            defer { saveTask = nil }
            do {
                switch try await discogs.saveDiscogsToken(token) {
                case .rejected:
                    wasRejected = true
                case .unvalidated, .valid:
                    // The status reaches the row through the config event.
                    break
                }
            }
            catch is CancellationError {
                logger.debug("saveToken cancelled")
            }
            catch {
                saveError = DisplayError(error)?
                    .rewording { line in
                        String(
                            localized: "Could not save the Discogs key: \(line)"
                        )
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
                saveError = DisplayError(error)?
                    .rewording { line in
                        String(
                            localized:
                                "Could not re-check the Discogs key: \(line)"
                        )
                    }
            }
        }
    }
}

// MARK: - DiscogsSettingsContent (pure leaf)

/// The key's rows under the Discogs switch: the field and Save, or what Discogs
/// said of the saved key, the check with Discogs while it runs, and any error
/// from the last action.
struct DiscogsSettingsContent: View {
    @Binding
    var draft: String
    let status: BridgeDiscogsTokenStatus
    /// Whether Search Discogs is switched on.
    let isEnabled: Bool
    let isValidating: Bool
    /// Whether Discogs turned down the key just saved, which core did not store.
    let wasRejected: Bool
    let saveError: DisplayError?
    let readError: DisplayError?
    let onSave: () -> Void
    let onRecheck: () -> Void

    /// Nothing to show: a working key, which the switch's row says, with no
    /// check running and nothing gone wrong.
    var isBlank: Bool {
        isEnabled && status == .valid && !isValidating && !wasRejected
            && saveError == nil && readError == nil
    }

    private var takesKey: Bool {
        !isEnabled || status == .notConfigured || status == .rejected
    }

    private var isInvalid: Bool {
        isEnabled && (status == .rejected || wasRejected)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            if takesKey {
                keyInput
            }
            if isInvalid {
                Notice(
                    title: String(localized: "Invalid key"),
                    advice: nil,
                    error: nil
                )
            }
            if isEnabled, status == .unvalidated {
                Notice(
                    title: String(localized: "Validation pending"),
                    advice: nil,
                    error: nil,
                    tone: .warning,
                    action: ErrorAction(
                        "Check now",
                        isDisabled: isValidating,
                        perform: onRecheck
                    )
                )
            }
            if isValidating {
                MacSpinner(
                    Text(
                        "Checking the key with Discogs…",
                        comment:
                            "Import settings, Discogs key: while bae asks Discogs whether the key works, after a save or a re-check."
                    )
                )
            }
            if let saveError {
                Notice(error: saveError)
            }
            if let readError {
                Notice(error: readError)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Key field and Save. The field takes the keyboard as it shows, and as
    /// the switch turns it on.
    private var keyInput: some View {
        HStack(spacing: ThemeSpace.compact) {
            KeyTextField(
                text: $draft,
                label: String(localized: "API key"),
                prompt: String(localized: "Your free Discogs key")
            )
            Button("Save", action: onSave)
                .disabled(draft.isEmpty || isValidating)
        }
        .disabled(!isEnabled)
    }
}

/// The Discogs key's field: plain text, which takes the keyboard as soon as
/// it is in a window while enabled, and again each time it is enabled.
private struct KeyTextField: NSViewRepresentable {
    @Binding
    var text: String
    let label: String
    let prompt: String

    func makeNSView(context: Context) -> FocusingTextField {
        let field = FocusingTextField()
        field.placeholderString = prompt
        field.setAccessibilityLabel(label)
        field.bezelStyle = .roundedBezel
        field.isBezeled = true
        field.delegate = context.coordinator
        field.cell?.usesSingleLineMode = true
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        field.stringValue = text
        field.focus = context.coordinator.focus
        let enabled = context.environment.isEnabled
        context.coordinator.wasEnabled = enabled
        field.isEnabled = enabled
        if enabled {
            context.coordinator.focus.request()
        }
        return field
    }

    func updateNSView(_ field: FocusingTextField, context: Context) {
        context.coordinator.text = $text
        let enabled = context.environment.isEnabled
        // SwiftUI sets the control's `isEnabled` itself before this runs, so
        // the field cannot say whether the switch just turned it on.
        let turnedOn = enabled && !context.coordinator.wasEnabled
        context.coordinator.wasEnabled = enabled
        field.isEnabled = enabled
        if turnedOn {
            context.coordinator.focus.request()
        }
        if field.stringValue != text {
            field.stringValue = text
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var text: Binding<String>
        let focus = FieldFocus()
        /// Whether the last update left the field enabled.
        var wasEnabled = false

        init(text: Binding<String>) {
            self.text = text
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else {
                return
            }
            text.wrappedValue = field.stringValue
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// The key as the Sources box draws it: the rows under the Discogs switch.
    struct DiscogsKeyPreview: View {
        let status: BridgeDiscogsTokenStatus
        var isEnabled = true
        var draft: String = ""
        var isValidating = false
        var wasRejected = false
        var saveError: DisplayError?

        var body: some View {
            Form {
                Section {
                    DiscogsSettingsContent(
                        draft: .constant(draft),
                        status: status,
                        isEnabled: isEnabled,
                        isValidating: isValidating,
                        wasRejected: wasRejected,
                        saveError: saveError,
                        readError: nil,
                        onSave: {},
                        onRecheck: {},
                    )
                }
            }
            .formStyle(.grouped)
            .frame(width: 500, height: 320)
        }
    }

    #Preview("Off") {
        DiscogsKeyPreview(status: .notConfigured, isEnabled: false)
    }

    #Preview("On, no key") {
        DiscogsKeyPreview(status: .notConfigured)
    }

    #Preview("Offline") {
        DiscogsKeyPreview(status: .unvalidated, draft: "abcdef123456")
    }

    #Preview("Checking") {
        DiscogsKeyPreview(
            status: .notConfigured,
            draft: "abcdef123456",
            isValidating: true
        )
    }

    #Preview("Invalid") {
        DiscogsKeyPreview(status: .rejected, draft: "bad-key")
    }
#endif
