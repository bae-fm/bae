import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeView")

struct WelcomeView: View {
    let onLibraryReady: (BridgeLibrary) -> Void

    enum Mode {
        case choose
        case restore
    }

    @State
    private var mode: Mode

    /// Default initializer used by first-run flow. Lands on `.choose`.
    init(onLibraryReady: @escaping (BridgeLibrary) -> Void) {
        self.onLibraryReady = onLibraryReady
        self._mode = State(initialValue: .choose)
    }

    /// Initializer used by the sidebar's "+ Add..." menu when the user
    /// has already picked a specific flow (Restore from code). Skips the
    /// chooser and lands directly on the requested mode.
    init(
        onLibraryReady: @escaping (BridgeLibrary) -> Void,
        initialMode: Mode
    ) {
        self.onLibraryReady = onLibraryReady
        self._mode = State(initialValue: initialMode)
    }

    @State
    private var isCreating = false
    @State
    private var error: String?

    // Restore code flow
    @State
    private var restoreCodeInput = ""
    /// The decode of the current restore-code input: `nil` when the input is
    /// empty (nothing to decode), `.success(info)` for a valid code, or
    /// `.failure(error)` describing why the input couldn't be parsed.
    @State
    private var decodedRestore: Result<BridgeRestoreCodeInfo, Error>?
    @State
    private var isRestoring = false
    /// The in-flight restore (from code or manual), owned so a superseding
    /// restore and the view's disappear can cancel it.
    @State
    private var restoreTask: Task<Void, Never>?
    @State
    private var oauthTokenJson: String?
    @State
    private var isAuthorizing = false
    @State
    private var showManualForm = false

    // Manual restore form fields
    @State
    private var restoreProvider: BridgeCloudProvider = .s3
    @State
    private var libraryId = ""
    @State
    private var libraryName = ""
    @State
    private var encryptionKey = ""
    // S3
    @State
    private var bucket = ""
    @State
    private var region = ""
    @State
    private var endpoint = ""
    @State
    private var accessKey = ""
    @State
    private var secretKey = ""
    /// Google Drive
    @State
    private var googleDriveFolderId = ""
    /// Dropbox
    @State
    private var dropboxFolderPath = ""
    // OneDrive
    @State
    private var oneDriveDriveId = ""
    @State
    private var oneDriveFolderId = ""

    /// Libraries already on this device, discovered on appear. Listed first as
    /// the primary "open" path; reopening after a close lands here.
    @State
    private var localLibraries: [BridgeLibrary] = []

    /// iCloud Keychain restore
    @State
    private var keychainEntries: [(code: String, info: BridgeRestoreCodeInfo)] =
        []
    @State
    private var deleteConfirmCode: String?

    /// Keychain restore codes whose library isn't already on this device.
    /// On-device libraries open directly (the `localLibraries` section), so
    /// the restore section only offers the ones that need a cloud pull.
    private var restorableEntries: [(code: String, info: BridgeRestoreCodeInfo)]
    {
        keychainEntries.filter { entry in
            !localLibraries.contains { $0.id == entry.info.libraryId }
        }
    }

    /// Whether any library is already available to open or restore — drives
    /// whether "Create new library" is the prominent first-run action or one
    /// option among several.
    private var hasExistingLibraries: Bool {
        !localLibraries.isEmpty || !restorableEntries.isEmpty
    }

    var body: some View {
        switch mode {
        case .choose:
            chooseView
        case .restore:
            restoreView
        }
    }

    private var chooseView: some View {
        VStack(spacing: 32) {
            Spacer()
            Text("bae")
                .font(.system(size: 48, weight: .bold, design: .rounded))
            Text("Get started with your music library.")
                .font(.title3)
                .foregroundStyle(.secondary)
            if !localLibraries.isEmpty {
                localLibrarySection
            }
            if !restorableEntries.isEmpty {
                keychainRestoreSection
            }
            VStack(spacing: 12) {
                if !hasExistingLibraries {
                    Button(action: doCreate) {
                        if isCreating {
                            ProgressView()
                                .controlSize(.small)
                        }
                        else {
                            Text("Create new library")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isCreating || isRestoring)
                    .keyboardShortcut(.defaultAction)
                }
                else {
                    Button(action: doCreate) {
                        if isCreating {
                            ProgressView()
                                .controlSize(.small)
                        }
                        else {
                            Text("Create new library")
                        }
                    }
                    .buttonStyle(.bordered)
                    .disabled(isCreating || isRestoring)
                }
                Button(action: { mode = .restore }) {
                    Text("Restore from cloud")
                }
                .buttonStyle(.bordered)
                .disabled(isCreating || isRestoring)
            }
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
            Spacer()
        }
        .padding()
        .onAppear {
            loadLocalLibraries()
            checkKeychainForRestoreCodes()
        }
        .onDisappear { restoreTask?.cancel() }
    }

    /// Libraries already on this device — the primary "open" path. A row per
    /// library opens it directly via `onLibraryReady`, the same callback the
    /// restore and create flows hand a ready library to.
    private var localLibrarySection: some View {
        VStack(spacing: 12) {
            Text(
                localLibraries.count == 1 ? "Your library" : "Your libraries"
            )
            .font(.headline)
            ForEach(localLibraries, id: \.id) { library in
                Button {
                    onLibraryReady(library)
                } label: {
                    Text(library.name)
                        .font(.body.bold())
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                }
                .buttonStyle(.bordered)
                .disabled(isCreating || isRestoring)
            }
        }
        .frame(maxWidth: 320)
    }

    private var keychainRestoreSection: some View {
        VStack(spacing: 12) {
            let entries = restorableEntries
            HStack(spacing: 4) {
                Text(
                    entries.count == 1
                        ? "Restore your library" : "Restore your libraries"
                )
                .font(.headline)
                InfoTip(
                    text: "Found from a previous setup on this Mac.",
                    learnMoreURL: URL(string: "https://bae.fm/sync/restore"),
                )
            }
            ForEach(Array(entries.enumerated()), id: \.offset) {
                _,
                entry in
                VStack(spacing: 8) {
                    HStack(spacing: 8) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(entry.info.libraryName)
                                .font(.body.bold())
                            Text(entry.info.cloudProviderLabel)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        // Restoring, authorizing, and idle controls all stay in
                        // the row's layout tree, toggled by opacity, so the row
                        // height doesn't change as the keychain entry's state
                        // flips between them.
                        let needsConnect =
                            entry.info.needsOauth && oauthTokenJson == nil
                        let idle = !isRestoring && !isAuthorizing
                        ZStack(alignment: .trailing) {
                            ProgressView()
                                .controlSize(.small)
                                .opacity(isRestoring ? 1 : 0)
                                .allowsHitTesting(false)

                            HStack(spacing: 8) {
                                ProgressView()
                                    .controlSize(.small)
                                Button("Cancel") {
                                    #if BAE_OAUTH_PROVIDERS
                                        oauthCancel()
                                    #endif
                                    isAuthorizing = false
                                }
                                .buttonStyle(.borderless)
                                .font(.callout)
                            }
                            .opacity(isAuthorizing ? 1 : 0)
                            .allowsHitTesting(isAuthorizing)

                            HStack(spacing: 8) {
                                ZStack(alignment: .trailing) {
                                    Button(
                                        "Connect \(entry.info.cloudProviderLabel)"
                                    ) {
                                        restoreCodeInput = entry.code
                                        decodedRestore = .success(entry.info)
                                        #if BAE_OAUTH_PROVIDERS
                                            doOAuthAuthorize(
                                                provider: entry.info
                                                    .cloudProvider
                                            )
                                        #endif
                                    }
                                    // Disabled (not just hidden) when it isn't
                                    // the active control, so it can't take Tab
                                    // focus while invisible.
                                    .disabled(!needsConnect)
                                    .opacity(needsConnect ? 1 : 0)
                                    .allowsHitTesting(needsConnect)

                                    Button("Restore") {
                                        restoreCodeInput = entry.code
                                        decodedRestore = .success(entry.info)
                                        doRestoreFromCode()
                                    }
                                    .buttonStyle(.borderedProminent)
                                    .keyboardShortcut(.defaultAction)
                                    // Disabled (not just hidden) when Connect is
                                    // the active control or a restore is running,
                                    // so the default-action shortcut can't fire
                                    // the hidden button on Enter.
                                    .disabled(needsConnect || !idle)
                                    .opacity(needsConnect ? 0 : 1)
                                    .allowsHitTesting(!needsConnect)
                                }
                                Button(role: .destructive) {
                                    deleteConfirmCode = entry.code
                                } label: {
                                    Image(systemName: "trash")
                                        .font(.callout)
                                }
                                .buttonStyle(.borderless)
                            }
                            .opacity(idle ? 1 : 0)
                            .allowsHitTesting(idle)
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Color.secondary.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
        }
        .frame(maxWidth: 320)
        .confirmationDialog(
            "Remove this library from your keychain?",
            isPresented: Binding(
                get: { deleteConfirmCode != nil },
                set: { if !$0 { deleteConfirmCode = nil } },
            ),
            titleVisibility: .visible,
        ) {
            Button("Remove", role: .destructive) {
                if let code = deleteConfirmCode,
                    let entry = keychainEntries.first(where: { $0.code == code }
                    )
                {
                    KeychainService.deleteRestoreCode(
                        libraryId: entry.info.libraryId
                    )
                    keychainEntries.removeAll { $0.code == code }
                }
                deleteConfirmCode = nil
            }
        } message: {
            Text(
                "You will not be able to recover this library without a restore code."
            )
        }
    }

    /// Decode a non-empty restore code into its info, or the error explaining
    /// why it couldn't be parsed. The caller owns the empty-input precondition —
    /// this always attempts a real decode.
    private func decode(
        restoreCode raw: String
    ) -> Result<BridgeRestoreCodeInfo, Error> {
        Result { try decodeRestoreCode(code: raw) }
    }

    private func loadLocalLibraries() {
        do {
            localLibraries = try discoverLibraries()
        }
        catch {
            logger.warning(
                "Skipping local library discovery: \(error.localizedDescription, privacy: .public)"
            )
            localLibraries = []
        }
    }

    private func checkKeychainForRestoreCodes() {
        let stored = KeychainService.fetchAllRestoreCodes()
        var decoded: [(code: String, info: BridgeRestoreCodeInfo)] = []
        for entry in stored {
            do {
                let info = try decodeRestoreCode(code: entry.code)
                decoded.append((code: entry.code, info: info))
            }
            catch {
                logger.warning(
                    "Skipping unreadable keychain restore entry: \(error.localizedDescription, privacy: .public)"
                )
            }
        }
        keychainEntries = decoded
    }

    private var restoreView: some View {
        VStack(spacing: 0) {
            Text("Restore from cloud")
                .font(.title2.bold())
                .padding(.top, 24)
                .padding(.bottom, 4)
            Text("Paste your restore code, or enter details manually.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(.bottom, 16)
            Form {
                Section("Restore code") {
                    TextField("Paste restore code", text: $restoreCodeInput)
                        .font(.system(.body, design: .monospaced))
                        .onChange(of: restoreCodeInput) { _, newInput in
                            oauthTokenJson = nil
                            let trimmed = newInput.trimmingCharacters(
                                in: .whitespaces
                            )
                            decodedRestore =
                                trimmed.isEmpty
                                ? nil
                                : decode(restoreCode: newInput)
                        }
                    if case .success(let info) = decodedRestore {
                        LabeledContent(
                            "Provider",
                            value: info.cloudProviderLabel
                        )
                        LabeledContent("Library", value: info.libraryName)
                        #if BAE_OAUTH_PROVIDERS
                            if info.needsOauth {
                                restoreCodeOauthRow(
                                    provider: info.cloudProvider
                                )
                            }
                        #endif
                    }
                    else if case .failure(let decodeError) = decodedRestore {
                        Text(decodeError.localizedDescription)
                            .foregroundStyle(.red)
                            .font(.callout)
                    }
                }
                DisclosureGroup(
                    "Enter details manually",
                    isExpanded: $showManualForm
                ) {
                    manualRestoreFields
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
                    .padding(.horizontal)
                    .padding(.bottom, 8)
            }
            if isRestoring {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Restoring library...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, 12)
            }
            HStack(spacing: 12) {
                Button("Back") {
                    mode = .choose
                    error = nil
                }
                .buttonStyle(.bordered)
                .disabled(isRestoring)
                Button("Restore") {
                    if case .success = decodedRestore {
                        doRestoreFromCode()
                    }
                    else {
                        doRestoreManual()
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(isRestoring || !restoreReady)
                .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, 24)
        }
        .padding(.horizontal)
        .onDisappear { restoreTask?.cancel() }
    }

    #if BAE_OAUTH_PROVIDERS
        private func restoreCodeOauthRow(
            provider: BridgeCloudProvider
        ) -> some View {
            HStack {
                if oauthTokenJson != nil {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                    Text("Connected")
                        .foregroundStyle(.secondary)
                }
                else if isAuthorizing {
                    ProgressView()
                        .controlSize(.small)
                    Text("Authorizing...")
                        .foregroundStyle(.secondary)
                    Button("Cancel") {
                        isAuthorizing = false
                    }
                    .buttonStyle(.borderless)
                    .font(.callout)
                }
                else {
                    Button("Connect \(cloudProviderLabel(provider: provider))")
                    {
                        doOAuthAuthorize(provider: provider)
                    }
                }
            }
        }
    #endif

    @ViewBuilder
    private var manualRestoreFields: some View {
        // The provider choices come from the compiled-in set, so a baeium
        // (S3-only) build offers just S3 and never references an OAuth/CloudKit
        // bridge symbol that isn't there.
        Picker("Cloud provider", selection: $restoreProvider) {
            ForEach(availableCloudProviders(), id: \.self) { provider in
                Text(cloudProviderLabel(provider: provider)).tag(provider)
            }
        }
        .onChange(of: restoreProvider) {
            oauthTokenJson = nil
        }
        TextField("Library ID", text: $libraryId)
            .textContentType(.none)
            .help("The UUID from your other device's library")
        SecureField("Encryption Key", text: $encryptionKey)
            .help("64-character hex-encoded encryption key")
        TextField("Library Name (optional)", text: $libraryName)
        manualProviderFields
    }

    @ViewBuilder
    private var manualProviderFields: some View {
        switch restoreProvider {
        case .s3:
            TextField("Bucket", text: $bucket)
            TextField("Region", text: $region)
            TextField("Endpoint (optional)", text: $endpoint)
                .help("Leave empty for standard AWS S3")
            SecureField("Access Key", text: $accessKey)
            SecureField("Secret Key", text: $secretKey)
        case .cloudKit:
            EmptyView()
        case .googleDrive:
            #if BAE_OAUTH_PROVIDERS
                manualOauthConnectRow
                if oauthTokenJson != nil {
                    TextField("Folder ID", text: $googleDriveFolderId)
                        .help(
                            "The Google Drive folder ID containing your library"
                        )
                }
            #else
                EmptyView()
            #endif
        case .dropbox:
            #if BAE_OAUTH_PROVIDERS
                manualOauthConnectRow
                if oauthTokenJson != nil {
                    TextField("Folder Path", text: $dropboxFolderPath)
                        .help("e.g. /Apps/bae/My Library")
                }
            #else
                EmptyView()
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                manualOauthConnectRow
                if oauthTokenJson != nil {
                    TextField("Drive ID", text: $oneDriveDriveId)
                    TextField("Folder ID", text: $oneDriveFolderId)
                }
            #else
                EmptyView()
            #endif
        }
    }

    #if BAE_OAUTH_PROVIDERS
        private var manualOauthConnectRow: some View {
            HStack {
                if oauthTokenJson != nil {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                    Text("Connected")
                        .foregroundStyle(.secondary)
                }
                else if isAuthorizing {
                    ProgressView()
                        .controlSize(.small)
                    Text("Authorizing...")
                        .foregroundStyle(.secondary)
                    Button("Cancel") {
                        isAuthorizing = false
                    }
                    .buttonStyle(.borderless)
                    .font(.callout)
                }
                else {
                    Button(
                        "Connect \(cloudProviderLabel(provider: restoreProvider))"
                    ) {
                        doOAuthAuthorize(provider: restoreProvider)
                    }
                }
            }
        }
    #endif

    // MARK: - Validation

    /// Whether the restore button should be enabled.
    private var restoreReady: Bool {
        if case .success(let info) = decodedRestore {
            // Restore code flow: valid code + OAuth done if needed
            if info.needsOauth {
                return oauthTokenJson != nil
            }
            return true
        }
        // Manual form flow
        if showManualForm {
            return manualFormValid
        }
        return false
    }

    private var manualFormValid: Bool {
        validateRestoreConfig(fields: buildRestoreFormFields())
    }

    private func buildRestoreFormFields() -> BridgeRestoreFormFields {
        switch restoreProvider {
        case .s3:
            .s3(
                libraryId: libraryId,
                encryptionKey: encryptionKey,
                bucket: bucket,
                region: region,
                accessKey: accessKey,
                secretKey: secretKey,
            )
        case .googleDrive:
            .googleDrive(
                libraryId: libraryId,
                encryptionKey: encryptionKey,
                folderId: googleDriveFolderId,
                hasOauthToken: oauthTokenJson != nil,
            )
        case .dropbox:
            .dropbox(
                libraryId: libraryId,
                encryptionKey: encryptionKey,
                folderPath: dropboxFolderPath,
                hasOauthToken: oauthTokenJson != nil,
            )
        case .oneDrive:
            .oneDrive(
                libraryId: libraryId,
                encryptionKey: encryptionKey,
                driveId: oneDriveDriveId,
                folderId: oneDriveFolderId,
                hasOauthToken: oauthTokenJson != nil,
            )
        case .cloudKit:
            .cloudKit(libraryId: libraryId, encryptionKey: encryptionKey)
        }
    }

    // MARK: - Actions

    private func doCreate() {
        isCreating = true
        error = nil
        Task.detached {
            do {
                let info = try createLibrary(name: nil)
                await MainActor.run {
                    isCreating = false
                    onLibraryReady(info)
                }
            }
            catch {
                await MainActor.run {
                    isCreating = false
                    self.error = error.localizedDescription
                }
            }
        }
    }

    /// Restore the library from the current restore-code input. The bridge
    /// re-decodes the code, so callers only need to have confirmed a valid
    /// decode first — there's nothing to pass in.
    private func doRestoreFromCode() {
        let code = restoreCodeInput
        let token = oauthTokenJson
        runRestore {
            try restoreFromCode(code: code, oauthTokenJson: token)
        }
    }

    private func doRestoreManual() {
        guard let source = buildRestoreSource() else {
            return
        }
        let lid = libraryId
        let ek = encryptionKey
        let name: String? = libraryName.isEmpty ? nil : libraryName
        runRestore {
            try restoreFromCloud(
                libraryId: lid,
                encryptionKeyHex: ek,
                libraryName: name,
                source: source,
            )
        }
    }

    /// Run a restore (from code or manual) off the UI thread, cancelling any
    /// in-flight restore first. The heavy bridge call blocks its worker, so the
    /// owned task is checked for cancellation before it touches `screen`-driving
    /// state: a superseded restore neither opens its (now stale) library nor
    /// clears `isRestoring` out from under the restore that replaced it.
    private func runRestore(
        _ work: @escaping @Sendable () throws -> BridgeLibrary
    ) {
        restoreTask?.cancel()
        isRestoring = true
        error = nil
        restoreTask = Task {
            do {
                let restored = try await DetachedWork.run(work)
                try Task.checkCancellation()
                isRestoring = false
                onLibraryReady(restored)
            }
            catch is CancellationError {
                // Superseded by a newer restore, which set `isRestoring = true`
                // for itself when it cancelled this one — leave the flag alone so
                // its spinner stays up. The superseding restore owns it.
                logger.debug("Restore superseded by a newer restore; skipping")
            }
            catch {
                isRestoring = false
                self.error = error.localizedDescription
            }
        }
    }

    private func buildRestoreSource() -> BridgeRestoreSource? {
        switch restoreProvider {
        case .s3:
            return .s3(
                bucket: bucket,
                region: region,
                endpoint: endpoint.isEmpty ? nil : endpoint,
                accessKey: accessKey,
                secretKey: secretKey,
            )
        case .cloudKit:
            return .cloudKit
        case .googleDrive:
            guard let token = oauthTokenJson else {
                error = "OAuth token required for Google Drive"
                return nil
            }
            return .googleDrive(
                folderId: googleDriveFolderId,
                oauthTokenJson: token
            )
        case .dropbox:
            guard let token = oauthTokenJson else {
                error = "OAuth token required for Dropbox"
                return nil
            }
            return .dropbox(
                folderPath: dropboxFolderPath,
                oauthTokenJson: token
            )
        case .oneDrive:
            guard let token = oauthTokenJson else {
                error = "OAuth token required for OneDrive"
                return nil
            }
            return .oneDrive(
                driveId: oneDriveDriveId,
                folderId: oneDriveFolderId,
                oauthTokenJson: token,
            )
        }
    }

    #if BAE_OAUTH_PROVIDERS
        private func doOAuthAuthorize(provider: BridgeCloudProvider) {
            isAuthorizing = true
            error = nil
            Task.detached {
                do {
                    let tokenJson = try oauthAuthorize(provider: provider)
                    await MainActor.run {
                        guard isAuthorizing else {
                            return
                        }
                        isAuthorizing = false
                        oauthTokenJson = tokenJson
                    }
                }
                catch {
                    await MainActor.run {
                        isAuthorizing = false
                        self.error = error.localizedDescription
                    }
                }
            }
        }
    #endif

}
