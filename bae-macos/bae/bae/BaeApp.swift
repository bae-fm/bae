import BaeKit
import Sparkle
import SwiftUI
import os.log

private let logger = Logger.bae("BaeApp")
private let appProcessEnvironment = ProcessInfo.processInfo.environment
private let appEdition: AppEdition = {
    #if BAE_OAUTH_PROVIDERS
        .bae
    #else
        .baeium
    #endif
}()

private enum AppRuntime {
    static func skipsApplicationServices(environment: [String: String]) -> Bool
    {
        isPreview(environment: environment)
            || isTestHost(environment: environment)
    }

    private static func isPreview(environment: [String: String]) -> Bool {
        environment["XCODE_RUNNING_FOR_PREVIEWS"] == "1"
    }

    private static func isTestHost(environment: [String: String]) -> Bool {
        #if DEBUG
            if environment["BAE_UI_TESTING"] == "1" {
                return false
            }
        #endif
        return environment["XCTestConfigurationFilePath"] != nil
    }

    #if DEBUG
        static func usesTestKeyring(environment: [String: String]) -> Bool {
            environment["BAE_UI_TESTING"] == "1"
        }

        static func createsLibraryForUITesting(
            environment: [String: String]
        ) -> Bool {
            environment["BAE_UI_TESTING_CREATE_LIBRARY"] == "1"
        }
    #endif
}

private let skipsApplicationServices = AppRuntime.skipsApplicationServices(
    environment: appProcessEnvironment
)

private func discoverInitialLibraries(
    environment: [String: String]
) throws -> [BridgeLibrary] {
    var libraries = try discoverLibraries()
    #if DEBUG
        if libraries.isEmpty,
            AppRuntime.createsLibraryForUITesting(environment: environment)
        {
            _ = try createLibrary(name: nil)
            libraries = try discoverLibraries()
        }
    #endif
    return libraries
}

enum AppScreen {
    case loading
    case welcome
    case unlock(libraryName: String)
    case keychainLocked(libraryId: String)
    case library
}

@main
struct BaeApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self)
    var appDelegate
    private let updaterController: SPUStandardUpdaterController
    private let librarySetup = LibrarySetup.live()
    @ObservedObject
    private var checkForUpdatesViewModel: CheckForUpdatesViewModel

    init() {
        let startUpdater = !AppRuntime.skipsApplicationServices(
            environment: appProcessEnvironment
        )
        #if DEBUG
            _ = startUpdater  // suppress unused warning
            let controller = SPUStandardUpdaterController(
                startingUpdater: false,
                updaterDelegate: nil,
                userDriverDelegate: nil
            )
        #else
            let controller = SPUStandardUpdaterController(
                startingUpdater: startUpdater,
                updaterDelegate: nil,
                userDriverDelegate: nil
            )
        #endif
        updaterController = controller
        checkForUpdatesViewModel = CheckForUpdatesViewModel(
            updater: controller.updater
        )
    }

    /// Window title — the active library's name if one is loaded, else
    /// just "bae". With multiple libraries on one device the title
    /// disambiguates which one the main window currently shows.
    private var windowTitle: String {
        if let name = appDelegate.appService?.libraryName, !name.isEmpty {
            return String(localized: "\(name) - bae")
        }
        return "bae"
    }

    /// WelcomeView constructed with the deep-link mode if a menu item
    /// requested one (Restore from Code), else the default chooser. Bound
    /// to the same callback for both paths, over the live pre-library
    /// operations — previews inject stubs through the same seam. The caller
    /// names the load error to surface: the bootstrap window passes the
    /// delegate's (a failed open belongs on the chooser), while the shell's
    /// Add Library sheet passes nil — a failed *switch* is the shell
    /// chrome's story, not the sheet's.
    private func welcomeView(loadError: DisplayError?) -> some View {
        Group {
            if let mode = appDelegate.welcomeInitialMode {
                WelcomeView(
                    onLibraryReady: { lib in appDelegate.openLibrary(lib) },
                    initialMode: mode,
                    canDeleteActiveLibrary: !appDelegate.hasShell
                )
            }
            else {
                WelcomeView(
                    onLibraryReady: { lib in appDelegate.openLibrary(lib) },
                    loadError: loadError,
                    canDeleteActiveLibrary: !appDelegate.hasShell
                )
            }
        }
        .environment(librarySetup)
    }

    /// Main-window content once the first library has opened. Switching
    /// libraries flips `screen` to `.loading` briefly; we render a
    /// ProgressView so the window shows progress across the swap.
    @ViewBuilder
    private var detailContent: some View {
        switch appDelegate.screen {
        case .loading:
            ProgressView("Switching libraries...")
        case .welcome:
            ProgressView()
        case .unlock(let libraryName):
            UnlockView(
                libraryName: libraryName,
                onUnlock: appDelegate.unlock,
                // Cancelling a switch-to-locked-library returns to the
                // library that's still open.
                onCancel: { appDelegate.screen = .library }
            )
        case .keychainLocked:
            KeychainLockedView(onRetry: appDelegate.retryKeychainOpen)
        case .library:
            MainAppView()
        }
    }

    /// Modal hosts attached to the main window: the welcome flow (New
    /// Library / Restore from Code), the rename sheet, and the lock
    /// confirmation. Each is driven by `AppDelegate` trigger state set from
    /// the File menu and acts through the active `AppService`.
    private func libraryModals<Content: View>(
        _ content: Content
    ) -> some View {
        content
            .sheet(
                isPresented: Binding(
                    get: { appDelegate.showAddLibrarySheet },
                    set: { appDelegate.showAddLibrarySheet = $0 }
                )
            ) {
                welcomeView(loadError: nil)
            }
            .sheet(
                item: Binding(
                    get: { appDelegate.renameLibrarySheet },
                    set: { appDelegate.renameLibrarySheet = $0 }
                )
            ) { sheet in
                RenameLibrarySheet(
                    // `sheet` is the item `.sheet(item:)` already unwrapped; use
                    // it as the fallback so the dismissal frame (when
                    // `renameLibrarySheet` has gone back to nil) shows the last
                    // real value rather than an empty-string sentinel.
                    state: Binding(
                        get: { appDelegate.renameLibrarySheet ?? sheet },
                        set: { appDelegate.renameLibrarySheet = $0 }
                    ),
                    onCancel: { appDelegate.renameLibrarySheet = nil },
                    onCommit: { newName in
                        appDelegate.renameLibrary(sheet.id, to: newName)
                    }
                )
            }
            .alert(
                "Lock library?",
                isPresented: Binding(
                    get: { appDelegate.confirmLockLibrary },
                    set: { appDelegate.confirmLockLibrary = $0 }
                )
            ) {
                Button("Lock", role: .destructive) {
                    appDelegate.lockActiveLibrary()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(lockConfirmMessage)
            }
    }

    /// Confirmation body for locking the active library, naming it so the
    /// user knows which one they're locking.
    private var lockConfirmMessage: String {
        let name =
            appDelegate.appService?.libraryName
            ?? String(localized: "This library")
        return String(
            localized:
                "\(name)'s encryption key will be removed from the keychain. This session keeps working; you'll need to re-enter the key on next launch."
        )
    }

    /// Pre-shell content: shown before any library has been opened.
    /// Loading spinner first, then `WelcomeView` if no libraries are
    /// discovered, then `UnlockView` if the auto-opened library is
    /// locked.
    @ViewBuilder
    private var bootstrapContent: some View {
        switch appDelegate.screen {
        case .loading:
            Spacer()
            ProgressView("Loading...")
            Spacer()
        case .welcome:
            welcomeView(loadError: appDelegate.loadError)
        case .unlock(let libraryName):
            UnlockView(
                libraryName: libraryName,
                onUnlock: appDelegate.unlock,
                // No shell yet (first launch, or opening from the welcome
                // chooser): cancelling returns to the welcome.
                onCancel: { appDelegate.screen = .welcome }
            )
        case .keychainLocked:
            KeychainLockedView(onRetry: appDelegate.retryKeychainOpen)
        case .library:
            // The service environment arrives with the library branch;
            // bootstrap content must remain independent of it.
            Spacer()
            ProgressView()
            Spacer()
        }
    }

    /// Opening adopts the library's preferred size before its shell mounts.
    /// The chooser and unlock flow use the fixed welcome size.
    private var bootstrapWindowSize: CGSize {
        if case .loading = appDelegate.screen {
            return MainWindow.defaultSize
        }
        return WelcomeWindow.size
    }

    var body: some Scene {
        primaryWindow
        storageManagerWindow
        settingsWindow
    }
}

extension BaeApp {
    @CommandsBuilder
    private var applicationCommands: some Commands {
        CommandGroup(after: .appInfo) {
            CheckForUpdatesView(viewModel: checkForUpdatesViewModel)
        }
        LibraryFileMenuCommands(
            libraries: appDelegate.libraries,
            onNewLibrary: { appDelegate.presentWelcome(mode: $0) },
            onOpenLibrary: { appDelegate.openLibrary($0) },
            onSwitchOffset: { appDelegate.switchLibrary(byOffset: $0) },
            onRenameLibrary: { appDelegate.presentRenameLibrary() },
            onLockLibrary: { appDelegate.presentLockLibraryConfirmation() },
            onSyncNow: { appDelegate.syncNow() },
            onRevealLibrary: { appDelegate.revealLibraryInFinder() },
            onCopyLibraryId: { appDelegate.copyLibraryId() },
            onCloseLibrary: { appDelegate.closeLibrary() }
        )
        MainAppMenuCommands()
    }

    /// One primary WindowGroup changes content and content-driven size as the
    /// library opens and closes; the window itself is never replaced.
    private var primaryWindow: some Scene {
        WindowGroup("bae", id: MainWindow.sceneID) {
            Group {
                if appDelegate.hasShell,
                    let appService = appDelegate.appService
                {
                    appService.installEnvironment(
                        libraryModals(
                            MainWindowChrome(loadError: appDelegate.loadError) {
                                detailContent
                            }
                            .navigationTitle(windowTitle)
                        )
                        // Window commands follow the focused library content;
                        // the WindowGroup itself keeps a stable identity.
                        .focusedSceneValue(
                            \.mainAppMenuTarget,
                            appService.mainAppMenuTarget
                        )
                    )
                }
                else {
                    WelcomeWindowChrome(size: bootstrapWindowSize) {
                        // A stack lets loading center its spinner with Spacers.
                        VStack(spacing: 0) {
                            bootstrapContent
                        }
                    }
                }
            }
            .navigationTitle(appDelegate.hasShell ? windowTitle : "bae")
            .onAppear {
                #if !DEBUG
                    updaterController.updater.checkForUpdatesInBackground()
                #endif
            }
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .defaultSize(
            width: MainWindow.defaultSize.width,
            height: MainWindow.defaultSize.height
        )
        .restorationBehavior(.disabled)
        // Launch and Dock reopen always present the primary app surface.
        .defaultLaunchBehavior(.presented)
        .commands { applicationCommands }
    }

    private var storageManagerWindow: some Scene {
        Window("Storage Manager", id: "storage-manager") {
            StorageManagerWindowRoot(appDelegate: appDelegate)
        }
        .defaultSize(width: 800, height: 500)
        // Never restored: a restored auxiliary window marks the session as
        // "already presented", which suppresses the primary window's launch
        // presentation — the app then opens with only this window, and every
        // quit re-saves that session. The primary window is the launch
        // surface; this one exists only through View → Storage Manager.
        .restorationBehavior(.disabled)
    }

    /// The Storage Manager window's root. A real `View` whose `body` does the
    /// `appService` read, so Observation tracks it: a window opened (or
    /// restored) before the library lands re-renders into the manager the
    /// moment the open completes, instead of freezing on the placeholder.
    private struct StorageManagerWindowRoot: View {
        let appDelegate: AppDelegate

        var body: some View {
            if let appService = appDelegate.appService {
                appService.installEnvironment(StorageManagerView())
            }
            else {
                ContentUnavailableView(
                    "No library loaded",
                    systemImage: "internaldrive",
                    description: Text("Open a library first")
                )
                .frame(width: 300, height: 200)
            }
        }
    }

    private var settingsWindow: some Scene {
        Settings {
            SettingsWindowRoot(
                appDelegate: appDelegate,
                checkForUpdatesViewModel: checkForUpdatesViewModel
            )
        }
        // Never restored, for the reason the Storage Manager window is not:
        // a restored auxiliary window marks the session as already presented
        // and suppresses the primary window. It is worse here — the settings
        // window is restored before the library opens, so it comes back
        // holding the no-library placeholder for a session that then has a
        // library, and Cmd+, just brings that stale window forward.
        .restorationBehavior(.disabled)
    }

    /// The settings window's root. A real `View` whose `body` does the
    /// `appService` read, so Observation tracks it — the same shape
    /// `StorageManagerWindowRoot` uses, and for the same reason. Read inline in
    /// the `Settings` scene builder instead, the read is not tracked: a window
    /// built before the library lands keeps the "No library loaded"
    /// placeholder for the rest of the run, while the menu bar and the title
    /// bar's gear open the very same scene and appear to disagree with it.
    private struct SettingsWindowRoot: View {
        let appDelegate: AppDelegate
        let checkForUpdatesViewModel: CheckForUpdatesViewModel

        var body: some View {
            if let appService = appDelegate.appService {
                appService.installEnvironment(
                    SettingsView(
                        checkForUpdatesViewModel: checkForUpdatesViewModel,
                        onForgetLibrary: { appDelegate.forgetActiveLibrary() }
                    )
                    .errorAlert(appDelegate.uiStore)
                    .onAppear { appService.reportScreen(.settings) }
                )
            }
            else {
                ContentUnavailableView(
                    "No library loaded",
                    systemImage: "books.vertical",
                    description: Text(
                        "Open a library first to access settings"
                    )
                )
                .frame(width: 300, height: 200)
            }
        }
    }
}

// MARK: - AppDelegate

@MainActor
@Observable
final class AppDelegate: NSObject, NSApplicationDelegate {
    var appService: AppService?
    private let mediaControlService = MediaControlService()
    /// The process-lifetime telemetry sink, built at delegate construction —
    /// before every other launch step (crash reporter, keyring, library open),
    /// so it exists for any failure they report. Held for the whole app run;
    /// every library open threads it into its `AppService`. The
    /// skip-application-services path (previews/tests) gets the no-op sink.
    private let diagnostics: BridgeDiagnostics =
        AppRuntime.skipsApplicationServices(environment: appProcessEnvironment)
        ? configureDiagnostics(config: .disabled)
        : BaeDiagnostics.configure(source: "macos", edition: appEdition)
    var uiStore = UiStore()
    var screen: AppScreen = .loading
    var loadError: DisplayError?
    /// When a menu item deep-links into a specific welcome flow (Restore
    /// from Code), this holds the requested initial mode for the welcome
    /// view about to be presented. `nil` lands on the default chooser.
    var welcomeInitialMode: WelcomeView.Mode?
    /// True once the first library has opened; the main window content
    /// replaces the bootstrap (loading / welcome / unlock) screens and stays
    /// up across switches.
    var hasShell: Bool = false
    /// Drives the welcome sheet presented by the New Library / Restore from
    /// Code menu items. Distinct from `.welcome` screen state, which only
    /// applies before any library has been opened.
    var showAddLibrarySheet: Bool = false
    /// Every library discovered on this device, kept current so the File →
    /// Open Library submenu (and its ⌘⇧1–9 switch shortcuts) always have the
    /// list without a view having to load it.
    var libraries: [BridgeLibrary] = []
    /// Non-nil while the Rename Library sheet is open; carries the target
    /// library id, the in-progress name, and any rename error.
    var renameLibrarySheet: RenameLibrarySheetState?
    /// Drives the Lock Library confirmation alert.
    var confirmLockLibrary: Bool = false
    /// The platform-shared open sequence. The factory reads `uiStore` fresh at
    /// build time (a close replaces it) and threads in the shared
    /// `mediaControlService`; the opener owns the supersede-cancel slot and maps
    /// each open to an `Outcome` this delegate lands on `screen`/`appService`.
    @ObservationIgnored
    private lazy var opener = LibrarySessionOpener<AppHandle, AppService>(
        // Capture the sink by value so the `@Sendable` makeHandle doesn't read
        // the main-actor property from off the main actor.
        makeHandle: { [diagnostics] libraryId in
            try initApp(
                libraryId: libraryId,
                positionUpdateIntervalMs: 200,
                // The "Restore on launch" preference: off starts with nothing
                // in playback; the core keeps the resume row current either way.
                restorePlayback: UserDefaults.standard.bool(
                    forKey: "persistPlayback"
                ),
                // The telemetry sink built at launch; `init_app` requires it, so
                // telemetry is guaranteed up before the library opens.
                diagnostics: diagnostics
            )
        },
        makeService: { [weak self] handle, config, initialOutbox in
            guard let self else {
                preconditionFailure("AppDelegate outlives its opener")
            }
            return self.makeService(
                handle: handle,
                uiStore: self.uiStore,
                config: config,
                initialOutbox: initialOutbox
            )
        }
    )
    /// In-flight library-list reload, cancelled when a newer one supersedes it.
    private let reloadSlot = CancellableTaskSlot()
    /// In-flight rename / lock, cancelled on library close.
    private let renameSlot = CancellableTaskSlot()
    private let lockSlot = CancellableTaskSlot()
    /// In-flight forget, cancelled on library close.
    private let forgetSlot = CancellableTaskSlot()
    /// The one graceful shutdown for the open library. Close and Quit share
    /// its result instead of starting competing shutdowns.
    @ObservationIgnored
    private let shutdownCoordinator =
        LibraryShutdownCoordinator<AppService>()
}

extension AppDelegate {
    // MARK: - Library lifecycle

    func presentWelcome(mode: WelcomeView.Mode?) {
        welcomeInitialMode = mode
        if hasShell {
            showAddLibrarySheet = true
        }
        else {
            screen = .welcome
        }
    }

    func presentRenameLibrary() {
        guard let appService else {
            preconditionFailure("Rename Library is disabled without a library")
        }
        renameLibrarySheet = RenameLibrarySheetState(
            id: appService.libraryId,
            newName: appService.libraryName
        )
    }

    func presentLockLibraryConfirmation() {
        guard appService != nil else {
            preconditionFailure("Lock Library is disabled without a library")
        }
        confirmLockLibrary = true
    }

    func syncNow() {
        guard let appService else {
            preconditionFailure("Sync Now is disabled without a library")
        }
        appService.triggerSync()
    }

    func revealLibraryInFinder() {
        guard let appService else {
            preconditionFailure("Reveal Library is disabled without a library")
        }
        SystemActions.revealInFinder(path: appService.libraryPath)
    }

    func copyLibraryId() {
        guard let appService else {
            preconditionFailure("Copy Library ID is disabled without a library")
        }
        SystemActions.copyToPasteboard(appService.libraryId)
    }

    /// Discover what is on this Mac and land on a screen.
    ///
    /// `canOpenLibraries` is false when launch already failed at something every
    /// open depends on (the keyring): discovery still runs — it reads config.yaml
    /// and needs no keychain — so the welcome screen lists the libraries that are
    /// here, but auto-opening one would only replace the recorded cause with the
    /// failure it produces.
    private func loadInitialState(canOpenLibraries: Bool) {
        if skipsApplicationServices {
            screen = .welcome
            return
        }
        do {
            let libraries = try discoverInitialLibraries(
                environment: appProcessEnvironment
            )
            self.libraries = libraries
            // Auto-open only a library whose config loaded. A broken one
            // (unreadable config.yaml) can't open — auto-trying it would just
            // strand its failure banner under the welcome screen, where the
            // chooser already shows the library with its error.
            guard canOpenLibraries,
                let openable = libraries.first(where: { $0.error == nil })
            else {
                screen = .welcome
                return
            }
            openLibrary(openable)
        }
        catch {
            // The welcome screen is where a launch with no library lands, and
            // it renders `loadError` as a callout. Staying on `.loading` left
            // the failure recorded and the user watching a spinner forever.
            loadError = DisplayError(error)
            screen = .welcome
        }
    }

    func openLibrary(_ library: BridgeLibrary) {
        openLocalLibrary(id: library.id)
    }

    func openLocalLibrary(id libraryId: String) {
        loadError = nil
        screen = .loading
        opener.open(libraryId: libraryId) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case .opened(let service):
                self.landOpenedService(service)
            case .needsUnlock(let config):
                self.screen = .unlock(libraryName: config.libraryName)
            case .keychainLocked:
                self.deferOpenForLockedKeychain(libraryId: libraryId)
            case .superseded:
                // Superseded by a newer open (or a close); that call owns
                // screen/appService.
                logger.debug(
                    "Library open superseded before it could land; skipping"
                )
            case .failed(let error):
                self.loadError = DisplayError(error)
                // A bootstrap open (first launch, or reopening from the welcome
                // chooser after a close) that fails must return to the welcome
                // so the user can retry or pick another, rather than strand on
                // the loading spinner. A switch failure keeps the shell mounted
                // and its own state.
                if !self.hasShell {
                    self.screen = .welcome
                }
            }
        }
    }

    func unlock(serializedCloudKey: String) async throws {
        let service = try await opener.unlock(
            serializedCloudKey: serializedCloudKey
        )
        landOpenedService(service)
    }

    private func landOpenedService(_ service: AppService) {
        appService = service
        screen = .library
        service.reportScreen(.library)
        reloadLibraries()
        hasShell = true
        showAddLibrarySheet = false
    }

    private func makeService(
        handle: AppHandle,
        uiStore: UiStore,
        config: BridgeConfig,
        initialOutbox: BridgeOutboxSnapshot
    ) -> AppService {
        let service = AppService(
            appHandle: handle,
            mediaControlService: mediaControlService,
            diagnostics: diagnostics,
            uiStore: uiStore,
            config: config,
            initialOutbox: initialOutbox
        )
        service.wireUp()
        return service
    }

    /// Close the open library and return to the welcome chooser after its
    /// graceful shutdown completes. A failure leaves the live service and
    /// shell in place so the user can see the error and retry.
    func closeLibrary() {
        guard let service = appService else { return }
        prepareForLibraryShutdown()
        screen = .loading
        beginLibraryShutdown(service)
    }

    @discardableResult
    private func beginLibraryShutdown(_ service: AppService)
        -> Task<LibraryShutdownResult, Never>
    {
        let attempt = shutdownCoordinator.begin(for: service) {
            try await service.shutdown()
        }
        if attempt.started {
            Task { [weak self, service, task = attempt.task] in
                let result = await task.value
                self?.finishLibraryShutdown(service, result: result)
            }
        }
        return attempt.task
    }

    private func finishLibraryShutdown(
        _ service: AppService,
        result: LibraryShutdownResult
    ) {
        guard shutdownCoordinator.hasPendingShutdown(for: service) else {
            return
        }
        shutdownCoordinator.finish(for: service)
        switch result {
        case .completed:
            releaseLibrarySession(service)
        case .failed(let failure):
            logger.error("Failed to shut down library: \(failure.diagnostic)")
            loadError = failure.displayedError
            screen = .library
        }
    }

    private func prepareForLibraryShutdown() {
        // Cancel any open still in flight so a parked `initApp` can't resume
        // past its post-await cancellation check and replace this session.
        opener.cancel()
        renameSlot.cancel()
        lockSlot.cancel()
        forgetSlot.cancel()
    }

    private func releaseLibrarySession(_ service: AppService) {
        guard appService === service else { return }
        service.deactivateMediaControls()
        appService = nil
        uiStore = UiStore()
        welcomeInitialMode = nil
        renameLibrarySheet = nil
        confirmLockLibrary = false
        loadError = nil
        screen = .welcome
        hasShell = false
    }

    /// Reload the device's library list off the main actor and publish it for
    /// the Open Library submenu. A newer reload cancels an in-flight one; on
    /// failure we log and keep the last good list rather than blanking the menu.
    func reloadLibraries() {
        reloadSlot.replace(
            "discoverLibraries",
            work: { try discoverLibraries() },
            onSuccess: { self.libraries = $0 },
            onError: {
                logger.error(
                    "Failed to list libraries: \($0.localizedDescription)"
                )
            }
        )
    }

    /// Switch to the library `offset` positions from the active one in the
    /// discovered list, wrapping around the ends. No-op with one (or no)
    /// library or no active library.
    /// Cycle to the next library. Broken ones are skipped: this opens without
    /// asking, and a library whose config won't load cannot be opened.
    func switchLibrary(byOffset offset: Int) {
        let openable = libraries.filter { $0.error == nil }
        guard openable.count > 1,
            let activeIdx = openable.firstIndex(where: \.isActive)
        else {
            return
        }
        let count = openable.count
        let next = ((activeIdx + offset) % count + count) % count
        openLibrary(openable[next])
    }

    /// Rename a library off the main actor, then refresh the list (and the
    /// window title, which reads the active library's name). On failure the
    /// error is written back into the open sheet; the sheet stays up so the
    /// user can retry.
    func renameLibrary(_ libraryId: String, to newName: String) {
        guard let appService else { return }
        let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        renameSlot.replace(
            "rename of \(libraryId)",
            work: { try appService.renameLibrary(libraryId, to: trimmed) },
            onSuccess: {
                self.renameLibrarySheet = nil
                self.reloadLibraries()
            },
            onError: {
                logger.error(
                    "Failed to rename \(libraryId): \($0.localizedDescription)"
                )
                self.renameLibrarySheet?.error = $0.localizedDescription
            }
        )
    }

    /// Lock the active library off the main actor: drop its encryption key from
    /// the keychain. The current session keeps working; the key is needed again
    /// on next launch. Errors surface through `loadError`.
    func lockActiveLibrary() {
        guard let appService else { return }
        lockSlot.replace(
            "lock",
            work: { try await appService.lockActiveLibrary() },
            onSuccess: {},
            onError: {
                logger.error(
                    "Failed to lock library: \($0.localizedDescription)"
                )
                self.loadError = DisplayError($0)
            }
        )
    }
}

extension AppDelegate {
    /// Remove the active library from this device: the bridge deletes its data
    /// directory, active-library pointer, and encryption key (the cloud copy,
    /// if the library syncs, is untouched), then the open service is torn down
    /// and the window returns to the welcome chooser. The bridge call must be
    /// the handle's last operation — the database lives in the removed
    /// directory — so teardown follows unconditionally on success. Errors
    /// surface through the global error alert and leave the library open. The
    /// master encryption key is dropped by core, not by a KeychainService call
    /// here; the restore-code entry is left intact so a synced library can be
    /// re-paired from the welcome screen.
    func forgetActiveLibrary() {
        guard let service = appService else {
            logger.warning(
                "Ignoring remove-library request: no library is open."
            )
            return
        }
        forgetSlot.replace(
            "forget",
            work: { try await service.forgetLibrary() },
            onSuccess: {
                // `forgetLibrary` is this handle's final operation because it
                // removes the database directory. Release it without asking
                // the deleted database to perform a later graceful shutdown.
                self.prepareForLibraryShutdown()
                self.releaseLibrarySession(service)
                self.reloadLibraries()
            },
            onError: {
                logger.error(
                    "Failed to remove library: \($0.localizedDescription)"
                )
                guard let displayed = DisplayError($0) else { return }
                self.uiStore.showError(
                    displayed.addingContext(
                        String(localized: "Couldn't remove library")
                    )
                )
            }
        )
    }
}

extension AppDelegate {
    func applicationDidFinishLaunching(_: Notification) {
        var keyringReady = true
        if !skipsApplicationServices {
            // Telemetry is already up (built at delegate construction, from
            // compiled-in values only), so every step from here on has a sink
            // for any failure it reports.
            BaeCrashReporting.configure(edition: appEdition)
            logger.info("application launched")
            do {
                try initializeKeyring()
            }
            catch {
                // Discovery reads config.yaml and needs no keychain, so a
                // keyring failure must not cost the user the list of libraries
                // on this Mac — returning here handed a first-run create/join
                // wall to someone with libraries sitting right there. It does
                // cost them the open, since every library's keys live in the
                // keyring, so discovery runs and the welcome screen lists them
                // under this error.
                logger.error(
                    "Keyring initialization failed: \(error.localizedDescription)"
                )
                loadError = DisplayError(error)
                keyringReady = false
            }
            // Hand Rust the CloudKit driver once. It can't build the driver
            // itself (it needs the platform CloudKit APIs); installing it is
            // idempotent and harmless for libraries that sync elsewhere, so it
            // belongs here at the composition root rather than at each open.
            #if BAE_CLOUDKIT
                setCloudkitDriver(driver: CloudKitService.bae())
            #endif
        }
        startWatchingForKeychainUnlock()
        loadInitialState(canOpenLibraries: keyringReady)
    }

    private func initializeKeyring() throws {
        #if DEBUG
            if AppRuntime.usesTestKeyring(
                environment: appProcessEnvironment
            ) {
                initTestKeyring()
                return
            }
        #endif
        try initKeyring(diagnostics: diagnostics)
    }

    /// AppKit's deferred-terminate flow, replacing the old fire-and-forget
    /// `applicationWillTerminate`: that method fired a detached `Task` and
    /// returned immediately, so the process could exit before the shutdown
    /// task's `persist_playback_state` write landed — losing the resume
    /// position on every quit that raced it. Returning `.terminateLater` here
    /// and replying only after `shutdown()` succeeds makes Quit wait for the
    /// save. A failure cancels termination and leaves the library open.
    func applicationShouldTerminate(_ sender: NSApplication)
        -> NSApplication.TerminateReply
    {
        guard let appService else {
            // No open library: closeLibrary() already shut its handle down
            // (or none was ever opened), so there's nothing left to save.
            return .terminateNow
        }
        prepareForLibraryShutdown()
        let shutdown = beginLibraryShutdown(appService)
        Task {
            let result = await shutdown.value
            let shouldTerminate: Bool
            switch result {
            case .completed:
                shouldTerminate = true
            case .failed:
                shouldTerminate = false
            }
            sender.reply(toApplicationShouldTerminate: shouldTerminate)
        }
        return .terminateLater
    }

    func applicationDidBecomeActive(_: Notification) {
        guard !skipsApplicationServices else { return }
        // Before the `appService` gate: having no service is the whole state.
        retryOpenIfKeychainWasLocked(trigger: "app activation")
        // Refresh the library list so the Open Library submenu reflects
        // libraries created, renamed, or removed elsewhere. Only meaningful
        // once a library is open — the menu that consumes it exists only then.
        guard appService != nil else { return }
        reloadLibraries()
    }

    func application(_: NSApplication, open urls: [URL]) {
        for url in urls {
            addWatchedFolderFromOpenURL(url)
        }
    }

    private func addWatchedFolderFromOpenURL(_ url: URL) {
        var isDir: ObjCBool = false
        guard
            FileManager.default.fileExists(
                atPath: url.path,
                isDirectory: &isDir
            ),
            isDir.boolValue
        else {
            return
        }
        Task {
            do {
                try await appService?.addWatchedFolder(path: url.path)
            }
            catch {
                guard let displayed = DisplayError(error) else { return }
                uiStore.showError(
                    displayed.addingContext(
                        String(localized: "Couldn't add folder")
                    )
                )
            }
        }
        uiStore.navigateToImport()
    }
}
