import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("Settings")

/// Appearance, library, sync, people, playback, and casting settings.
struct SettingsView: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(AppSessionHolder.self)
    private var holder
    @Environment(Sync.self)
    private var sync
    @Environment(Playback.self)
    private var playback
    @Environment(\.dismiss)
    private var dismiss

    // On by default; read at the next app launch.
    @AppStorage("persistPlayback")
    private var persistPlayback = true

    @State
    private var confirmRemove = false
    @State
    private var showRestoreCode = false

    var body: some View {
        NavigationStack {
            List {
                Section("Appearance") {
                    AppearanceControls()
                }
                if holder.hasMultipleLibraries {
                    LibrarySwitcherSection()
                }
                LibraryNameSection()

                if let location = configStore.storage {
                    SyncSection(location: location)
                }
                else {
                    Section("Sync") {
                        NavigationLink("Set Up Sync") {
                            StorageSetupScreen()
                        }
                    }
                }

                if configStore.hasCloudHome {
                    Section {
                        NavigationLink("People") {
                            MembersView()
                        }
                    } footer: {
                        Text(
                            "The people who share this library and their devices."
                        )
                    }
                    HistorySharingSection()
                    Section {
                        Button("Show restore code\u{2026}") {
                            showRestoreCode = true
                        }
                    } footer: {
                        Text(
                            "Your restore code adds your other devices to this library. Anyone with it and access to your storage can open the library. Keep it secret."
                        )
                    }
                }

                Section {
                    PauseBetweenSidesToggle(
                        configStore: configStore,
                        setEnabled: playback.setPauseBetweenSides,
                        showError: { @MainActor error in
                            configStore.showError(error)
                        }
                    )
                    SidePauseCountdownPicker(
                        configStore: configStore,
                        setCountdown: playback.setSidePauseCountdown,
                        showError: { @MainActor error in
                            configStore.showError(error)
                        }
                    )
                    Toggle("Restore on launch", isOn: $persistPlayback)
                } header: {
                    Text("Playback")
                } footer: {
                    Text(
                        "Restores the last session's track, position, queue, and volume when the app opens."
                    )
                }

                CastingSettingsSections()

                Section {
                    Button(role: .destructive) {
                        confirmRemove = true
                    } label: {
                        Text("Remove this library from this device")
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } footer: {
                    Text(removeFooter)
                }

                Section("About") {
                    LabeledContent("Version", value: Self.appVersion)
                }
            }
            .scrollContentBackground(.hidden)
            .windowBackground()
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .confirmationDialog(
                "Remove this library from this device?",
                isPresented: $confirmRemove,
                titleVisibility: .visible
            ) {
                Button("Remove", role: .destructive) {
                    dismiss()
                    holder.removeActiveLibrary()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(removeMessage)
            }
            .sheet(isPresented: $showRestoreCode) {
                RestoreCodeView(
                    generate: sync.generateRestoreCode,
                    onDismiss: { showRestoreCode = false }
                )
            }
            .alert(
                "Error",
                isPresented: Binding(
                    get: { configStore.lastError != nil },
                    set: { if !$0 { configStore.clearError() } },
                )
            ) {
                Button("Close") { configStore.clearError() }
            } message: {
                if let error = configStore.lastError {
                    Text(error.line)
                }
            }
            .onAppear { holder.reportScreen(.settings) }
        }
    }

    /// The remove section's footer, which depends on whether the library
    /// syncs.
    private var removeFooter: String {
        if configStore.hasCloudHome {
            return String(
                localized:
                    "Removes this device from the library's members, then deletes the library and its downloaded files from this device. Your library in the cloud is untouched. Removing needs a connection to your storage."
            )
        }
        return String(
            localized:
                "This library isn't synced. Removing it permanently deletes its catalog from this device."
        )
    }

    /// The confirmation's body: a synced library's cloud copy survives, and
    /// queued cloud writes are lost.
    private var removeMessage: String {
        guard configStore.hasCloudHome else { return removeFooter }
        let base = String(
            localized:
                "This device leaves the library, then its copy is deleted. Your library in the cloud is untouched."
        )
        guard outboxStore.hasPendingCloudWork else { return base }
        return base + " "
            + String(
                localized:
                    "Some changes haven't finished uploading and will be lost."
            )
    }
}

/// The libraries on this device, the open one checked; tapping another opens
/// it.
private struct LibrarySwitcherSection: View {
    @Environment(AppSessionHolder.self)
    private var holder

    var body: some View {
        Section("Library") {
            ForEach(holder.libraries, id: \.id) { library in
                Button {
                    holder.openLibrary(id: library.id)
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: ThemeSpace.line) {
                            Text(library.name)
                            // Every row keeps the detail line so rows share
                            // one height.
                            Text(library.detailLine)
                                .themeText(.detail)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        // Hidden, not removed, so switching libraries doesn't
                        // re-measure rows.
                        Image(systemName: "checkmark")
                            .foregroundStyle(Theme.accent)
                            .opacity(holder.isActive(library) ? 1 : 0)
                            .allowsHitTesting(false)
                    }
                    .contentShape(Rectangle())
                }
                .foregroundStyle(.primary)
                .disabled(holder.isActive(library))
            }
        }
    }
}

/// The library's name, which the household shares; admins rename it.
private struct LibraryNameSection: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(MembershipStore.self)
    private var membership
    @Environment(Sync.self)
    private var sync

    @State
    private var renaming = false
    @State
    private var newName = ""
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        Section {
            LabeledContent("Name", value: configStore.libraryName)
            if membership.selfIsAdmin {
                Button("Rename Library\u{2026}") {
                    newName = configStore.libraryName
                    renaming = true
                }
                .disabled(task != nil)
            }
            if let error {
                ErrorDetailDisclosure(error: error)
            }
        }
        .alert("Rename Library", isPresented: $renaming) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { rename() }
                .disabled(
                    newName.trimmingCharacters(in: .whitespaces).isEmpty
                )
        } message: {
            Text("Every device in the library shows the new name.")
        }
        .onDisappear { task?.cancel() }
    }

    private func rename() {
        let name = newName.trimmingCharacters(in: .whitespaces)
        error = nil
        task = Task {
            defer { task = nil }
            do { try await sync.renameLibrary(name) }
            catch is CancellationError {
                logger.debug("library rename cancelled")
            }
            catch {
                logger.error(
                    "Failed to rename the library: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

/// The Casting sections: one toggle that core enforces, with a warning before
/// turning it off would end a session, and, while casting is on, what a track
/// a device can't play directly is converted to.
private struct CastingSettingsSections: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore

    /// The device an unconfirmed "turn casting off" would disconnect from.
    @State
    private var pendingCastDisconnect: String?

    var body: some View {
        Section {
            Toggle("Enable casting", isOn: castEnabledBinding)
        } header: {
            Text("Casting")
        } footer: {
            Text(
                "Plays to Cast and AirPlay receivers on your network. While off, bae does not look for devices."
            )
        }
        .alert(
            "Turn off casting?",
            isPresented: Binding(
                get: { pendingCastDisconnect != nil },
                set: { presented in
                    if !presented { pendingCastDisconnect = nil }
                }
            ),
            presenting: pendingCastDisconnect
        ) { _ in
            Button("Turn Off", role: .destructive) { setCastEnabled(false) }
            Button("Cancel", role: .cancel) {}
        } message: { device in
            Text("This will stop casting to \(device).")
        }

        if CastTranscodeFormatPicker.isShown(for: configStore.config) {
            Section {
                CastTranscodeFormatPicker(
                    configStore: configStore,
                    setFormat: cast.setTranscodeFormat,
                    showError: { @MainActor error in
                        configStore.showError(error)
                    }
                )
            } footer: {
                Text(
                    "Tracks a Cast device can't play directly, such as tracks from a single file + CUE, are converted while they play. WAV is lossless but takes several times the data of MP3."
                )
            }
        }
    }

    /// Only the config subscription moves the switch, so a refused flip leaves
    /// it where it was.
    private var castEnabledBinding: Binding<Bool> {
        Binding(
            get: { configStore.config.castEnabled },
            set: { enabled in
                switch Cast.toggleAction(
                    enabled: enabled,
                    castingDeviceName: castStore.castingDeviceName
                ) {
                case .apply(let enabled): setCastEnabled(enabled)
                case .confirmDisconnect(let device):
                    pendingCastDisconnect = device
                }
            }
        )
    }

    private func setCastEnabled(_ enabled: Bool) {
        Task {
            do {
                try await cast.setEnabled(enabled)
            }
            catch {
                configStore.showError(error)
            }
        }
    }
}

extension SettingsView {
    /// The app's version and build, e.g. "1.2 (345)"; every build stamps both.
    fileprivate static var appVersion: String {
        let info = Bundle.main
        guard
            let short = info.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
            let build = info.object(forInfoDictionaryKey: "CFBundleVersion") as? String
        else {
            preconditionFailure("the app bundle always stamps its version keys")
        }
        return "\(short) (\(build))"
    }
}

#if DEBUG
#Preview {
    SettingsView()
        .previewStores()
}
#endif
