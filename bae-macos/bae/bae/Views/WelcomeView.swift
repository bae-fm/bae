import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeView")

struct WelcomeView: View {
    let onLibraryReady: (BridgeLibrary) -> Void

    enum Mode {
        case choose
        case restore
        case join
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

    // Join-a-library flow
    /// The cloud provider the joiner picked for the library they're adding this
    /// device to. `nil` until picked — while `nil` the join flow shows the
    /// provider picker and no code is generated yet. Picking an OAuth provider
    /// authenticates up front so the account email lands in the join-request;
    /// picking S3/iCloud generates the code with no email.
    @State
    private var joinProvider: BridgeCloudProvider?
    /// This device's join-request code and a short fingerprint of its public
    /// key, generated once a provider is picked (and, for OAuth, authenticated)
    /// and shown for an existing member to scan or paste. `nil` while generating;
    /// `.failure` if generation fails.
    @State
    private var joinRequest: Result<BridgeJoinRequest, Error>?
    /// The account email fetched from the picked OAuth provider, baked into the
    /// join-request so the approver shares the OAuth folder to it. `nil` for
    /// S3/iCloud, which share no folder. Held so a code retry reuses it without
    /// re-authenticating.
    @State
    private var joinEmail: String?
    /// The in-flight provider-prepare / (re)generation of this device's join
    /// code, owned so a retry supersedes the previous attempt and the view's
    /// disappear cancels it.
    @State
    private var genTask: Task<Void, Never>?
    @State
    private var inviteCodeInput = ""
    /// The decode of the current invite-code input: `nil` when empty,
    /// `.success(info)` for a valid code, `.failure(error)` for an unparseable
    /// one.
    @State
    private var decodedInvite: Result<BridgeInviteCodeInfo, Error>?
    @State
    private var isJoining = false
    /// The in-flight join, owned so a superseding join and the view's disappear
    /// can cancel it.
    @State
    private var joinTask: Task<Void, Never>?
    @State
    private var showInviteScanner = false

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
        case .join:
            joinView
        }
    }
}

// MARK: - Choose flow

extension WelcomeView {
    fileprivate var chooseView: some View {
        VStack(spacing: 32) {
            Spacer()
            Text(verbatim: "bae")
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
                Button(action: { mode = .join }) {
                    Text("Join a library")
                }
                .buttonStyle(.bordered)
                .disabled(isCreating || isRestoring)
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
        .task {
            await loadLocalLibraries()
        }
        .task {
            await checkKeychainForRestoreCodes()
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
                            Text(entry.info.cloudProvider.displayName)
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
                                        "Connect \(entry.info.cloudProvider.displayName)"
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

    private func loadLocalLibraries() async {
        do {
            let discovered = try await DetachedWork.run {
                try discoverLibraries()
            }
            try Task.checkCancellation()
            localLibraries = discovered
        }
        catch is CancellationError {
        }
        catch {
            logger.warning(
                "Skipping local library discovery: \(error.localizedDescription)"
            )
            localLibraries = []
        }
    }

    private func checkKeychainForRestoreCodes() async {
        do {
            let decoded = try await DetachedWork.run {
                let stored = KeychainService.fetchAllRestoreCodes()
                var decoded: [(code: String, info: BridgeRestoreCodeInfo)] =
                    []
                for entry in stored {
                    do {
                        let info = try decodeRestoreCode(code: entry.code)
                        decoded.append((code: entry.code, info: info))
                    }
                    catch {
                        logger.warning(
                            "Skipping unreadable keychain restore entry: \(error.localizedDescription)"
                        )
                    }
                }
                return decoded
            }
            try Task.checkCancellation()
            keychainEntries = decoded
        }
        catch is CancellationError {
        }
        catch {
            logger.warning(
                "Skipping keychain restore lookup: \(error.localizedDescription)"
            )
            keychainEntries = []
        }
    }
}

// MARK: - Restore flow

extension WelcomeView {
    fileprivate var restoreView: some View {
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
                            value: info.cloudProvider.displayName
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
                    Button("Connect \(provider.displayName)") {
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
                Text(provider.displayName).tag(provider)
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
                        "Connect \(restoreProvider.displayName)"
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

    fileprivate func doCreate() {
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
    fileprivate func doRestoreFromCode() {
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
                error = String(
                    localized: "OAuth token required for Google Drive"
                )
                return nil
            }
            return .googleDrive(
                folderId: googleDriveFolderId,
                oauthTokenJson: token
            )
        case .dropbox:
            guard let token = oauthTokenJson else {
                error = String(localized: "OAuth token required for Dropbox")
                return nil
            }
            return .dropbox(
                folderPath: dropboxFolderPath,
                oauthTokenJson: token
            )
        case .oneDrive:
            guard let token = oauthTokenJson else {
                error = String(localized: "OAuth token required for OneDrive")
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
        fileprivate func doOAuthAuthorize(provider: BridgeCloudProvider) {
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

// MARK: - Join flow

extension WelcomeView {
    fileprivate var joinView: some View {
        VStack(spacing: 0) {
            Text("Join a library")
                .font(.title2.bold())
                .padding(.top, 24)
                .padding(.bottom, 4)
            Text(
                "Add this device to a library you already have on another device."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.bottom, 16)

            if joinProvider == nil {
                joinProviderPicker
            }
            else {
                joinCodeExchange
            }
        }
        .padding(.horizontal)
        // Entering the join flow fresh: clear any provider/token connected in
        // another mode (or a prior join attempt) so nothing leaks across flows.
        // The code is generated only once the joiner picks a provider.
        .task {
            joinProvider = nil
            oauthTokenJson = nil
            joinRequest = nil
            error = nil
        }
        .onDisappear {
            genTask?.cancel()
            joinTask?.cancel()
        }
        .sheet(isPresented: $showInviteScanner) {
            InviteScannerSheet(
                onScan: { code in
                    inviteCodeInput = code
                    showInviteScanner = false
                },
                onDismiss: { showInviteScanner = false },
            )
        }
    }

    /// Step one of the join flow: pick the cloud provider the target library
    /// uses. For an OAuth provider this authenticates up front so the account
    /// email is baked into the join-request; for S3/iCloud it generates the code
    /// with no email. The picker offers the providers this build supports.
    private var joinProviderPicker: some View {
        VStack(spacing: 0) {
            Text(
                "Choose the cloud provider the library you're joining uses."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .padding(.bottom, 16)

            if isAuthorizing {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Signing in...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Button("Cancel") {
                        #if BAE_OAUTH_PROVIDERS
                            oauthCancel()
                        #endif
                        genTask?.cancel()
                        isAuthorizing = false
                    }
                    .buttonStyle(.borderless)
                    .font(.callout)
                }
                .padding(.bottom, 12)
            }
            else {
                VStack(spacing: 8) {
                    ForEach(availableCloudProviders(), id: \.self) { provider in
                        Button(provider.displayName) {
                            selectJoinProvider(provider)
                        }
                        .buttonStyle(.bordered)
                        .frame(maxWidth: .infinity)
                    }
                }
                .padding(.bottom, 8)
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
                    .padding(.horizontal)
                    .padding(.bottom, 8)
            }

            Button("Back") {
                mode = .choose
                error = nil
            }
            .buttonStyle(.bordered)
            .disabled(isAuthorizing)
            .padding(.bottom, 24)
        }
    }

    /// Step two of the join flow: show this device's code and take the invite
    /// code the approving device hands back. Reached only after a provider is
    /// picked, so the code is already generated (with the account email for an
    /// OAuth provider).
    private var joinCodeExchange: some View {
        VStack(spacing: 0) {
            Form {
                Section("This device's code") {
                    joinRequestRow
                }
                Section("Invite code") {
                    inviteCodeRow
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
            if isJoining {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Joining library...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, 12)
            }
            HStack(spacing: 12) {
                Button("Back") {
                    // Return to the provider picker; drop the generated code and
                    // any OAuth token so re-picking starts clean.
                    joinProvider = nil
                    joinRequest = nil
                    oauthTokenJson = nil
                    inviteCodeInput = ""
                    decodedInvite = nil
                    error = nil
                }
                .buttonStyle(.bordered)
                .disabled(isJoining)
                Button("Join") { doJoin() }
                    .buttonStyle(.borderedProminent)
                    .disabled(isJoining || !joinReady)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, 24)
        }
    }

    @ViewBuilder
    private var joinRequestRow: some View {
        switch joinRequest {
        case nil:
            ProgressView()
                .frame(maxWidth: .infinity)
        case .success(let generated):
            VStack(spacing: 12) {
                Text(
                    "On a device already in your library, open Settings → Members → Add a device and scan or paste this."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

                if let qrImage = QRCode.image(from: generated.code) {
                    Image(nsImage: qrImage)
                        .interpolation(.none)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 160, height: 160)
                }

                Text(generated.code)
                    .font(.system(.caption, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity)

                // The approving device shows this same fingerprint; matching them
                // confirms the right device is being added.
                Text("This device: \(generated.fingerprint)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)

                Button("Copy code") {
                    SystemActions.copyToPasteboard(generated.code)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 4)
        case .failure(let genError):
            VStack(spacing: 8) {
                Text(genError.localizedDescription)
                    .foregroundStyle(.red)
                    .font(.callout)
                Button("Try again") {
                    genTask?.cancel()
                    genTask = Task { await generateJoinCode() }
                }
            }
            .frame(maxWidth: .infinity)
        }
    }

    @ViewBuilder
    private var inviteCodeRow: some View {
        Text(
            "Once that device approves this one, enter the invite code it shows."
        )
        .font(.caption)
        .foregroundStyle(.secondary)

        HStack(spacing: 8) {
            TextField("Paste invite code", text: $inviteCodeInput)
                .font(.system(.body, design: .monospaced))
            Button("Scan") { showInviteScanner = true }
        }
        .onChange(of: inviteCodeInput) { _, newInput in
            let trimmed = newInput.trimmingCharacters(in: .whitespaces)
            decodedInvite =
                trimmed.isEmpty
                ? nil
                : Result { try decodeInviteCode(code: newInput) }
        }

        if case .success(let info) = decodedInvite {
            LabeledContent("Library", value: info.libraryName)
            LabeledContent(
                "Provider",
                value: info.cloudProvider.displayName
            )
            LabeledContent(
                "Owner",
                value: info.ownerFingerprint
            )
            #if BAE_OAUTH_PROVIDERS
                // OAuth is done up front at provider selection, so a token is
                // already held. A missing token here means the joiner picked a
                // provider that doesn't match this library — send them back.
                if info.needsOauth && oauthTokenJson == nil {
                    Text(
                        "This library needs a signed-in provider. Go back and choose the provider it uses."
                    )
                    .foregroundStyle(.red)
                    .font(.callout)
                }
            #else
                if info.needsOauth {
                    Text(
                        "This library uses a provider this build can't connect to."
                    )
                    .foregroundStyle(.red)
                    .font(.callout)
                }
            #endif
        }
        else if case .failure(let decodeError) = decodedInvite {
            Text(decodeError.localizedDescription)
                .foregroundStyle(.red)
                .font(.callout)
        }
    }

    /// Whether the Join button should be enabled: a valid invite code, plus the
    /// up-front OAuth token held when the picked provider needs it.
    private var joinReady: Bool {
        guard case .success(let info) = decodedInvite else {
            return false
        }
        if info.needsOauth {
            return oauthTokenJson != nil
        }
        return true
    }

    /// Pick the provider the target library uses. For an OAuth provider this
    /// authenticates and fetches the account email up front (baked into the
    /// generated code and reused for the eventual join); for S3/iCloud it goes
    /// straight to generating a code with no email.
    private func selectJoinProvider(_ provider: BridgeCloudProvider) {
        genTask?.cancel()
        error = nil
        oauthTokenJson = nil
        joinEmail = nil
        genTask = Task { await prepareJoin(provider: provider) }
    }

    private func prepareJoin(provider: BridgeCloudProvider) async {
        #if BAE_OAUTH_PROVIDERS
            switch provider {
            case .googleDrive, .dropbox, .oneDrive:
                isAuthorizing = true
                do {
                    let tokenJson = try await DetachedWork.run {
                        try oauthAuthorize(provider: provider)
                    }
                    let email = try await DetachedWork.run {
                        try fetchAccountEmail(
                            provider: provider,
                            oauthTokenJson: tokenJson
                        )
                    }
                    isAuthorizing = false
                    oauthTokenJson = tokenJson
                    joinEmail = email
                }
                catch is CancellationError {
                    isAuthorizing = false
                    return
                }
                catch {
                    isAuthorizing = false
                    self.error = error.localizedDescription
                    return
                }
            default:
                break
            }
        #endif
        joinProvider = provider
        await generateJoinCode()
    }

    private func generateJoinCode() async {
        joinRequest = nil
        let email = joinEmail
        do {
            let generated = try await DetachedWork.run {
                try generateJoinRequest(email: email)
            }
            joinRequest = .success(generated)
        }
        catch is CancellationError {
            logger.debug("join request generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate join request: \(error.localizedDescription)"
            )
            joinRequest = .failure(error)
        }
    }

    /// Join the library from the current invite code. The join runs through a
    /// cancellable operation: a superseding join (or the view disappearing)
    /// cancels the in-flight one through the operation's own token, so the bridge
    /// stops its blocking work rather than running to completion on a stale
    /// library. The post-await cancellation check guards the success path so a
    /// superseded join neither opens its stale library nor clears `isJoining`
    /// out from under the join that replaced it.
    private func doJoin() {
        let code = inviteCodeInput
        let token = oauthTokenJson
        joinTask?.cancel()
        isJoining = true
        error = nil
        joinTask = Task {
            let operation: JoinFromCodeOperation
            do {
                operation = try joinFromCodeOperation(
                    code: code,
                    oauthTokenJson: token
                )
            }
            catch {
                isJoining = false
                self.error = error.localizedDescription
                return
            }
            do {
                let detached = Task.detached { try operation.join() }
                let joined = try await withTaskCancellationHandler {
                    try await detached.value
                } onCancel: {
                    operation.cancel()
                    detached.cancel()
                }
                try Task.checkCancellation()
                isJoining = false
                onLibraryReady(joined)
            }
            catch is CancellationError {
                logger.debug("Join superseded by a newer join; skipping")
            }
            catch {
                isJoining = false
                self.error = error.localizedDescription
            }
        }
    }
}
