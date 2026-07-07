import Combine
import Sparkle
import SwiftUI
import os.log

private let logger = Logger.bae("BaeApp")
private let appProcessEnvironment = ProcessInfo.processInfo.environment

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
        environment["XCTestConfigurationFilePath"] != nil
    }
}

enum AppScreen {
    case loading
    case welcome
    case unlock(
        libraryId: String,
        libraryName: String,
        fingerprint: String?
    )
    case library
}

@main
struct BaeApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self)
    var appDelegate
    private let updaterController: SPUStandardUpdaterController
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
                userDriverDelegate: nil,
            )
        #else
            let controller = SPUStandardUpdaterController(
                startingUpdater: startUpdater,
                updaterDelegate: nil,
                userDriverDelegate: nil,
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
        if let name = appDelegate.appService?.configStore.config.libraryName,
            !name.isEmpty
        {
            return String(localized: "\(name) — bae")
        }
        return "bae"
    }

    private var playbackPublisher: AnyPublisher<PlaybackPositionEvent, Never> {
        appDelegate.appService?.playbackStore.playbackPositionSubject
            .eraseToAnyPublisher()
            ?? Empty().eraseToAnyPublisher()
    }

    private var previewPublisher: AnyPublisher<PreviewProgressEvent, Never> {
        appDelegate.appService?.importStore.previewProgressSubject
            .eraseToAnyPublisher()
            ?? Empty().eraseToAnyPublisher()
    }

    private var importLoudnessPublisher:
        AnyPublisher<ImportLoudnessProgressEvent?, Never>
    {
        appDelegate.appService?.importStore.importLoudnessSubject
            .eraseToAnyPublisher()
            ?? Empty().eraseToAnyPublisher()
    }

    /// WelcomeView constructed with the deep-link mode if a menu item
    /// requested one (Restore from Code), else the default chooser. Bound
    /// to the same callback for both paths.
    @ViewBuilder
    private var welcomeView: some View {
        if let mode = appDelegate.welcomeInitialMode {
            WelcomeView(
                onLibraryReady: { lib in appDelegate.openLibrary(lib) },
                initialMode: mode,
            )
        }
        else {
            WelcomeView { lib in appDelegate.openLibrary(lib) }
        }
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
        case .unlock(
            let libraryId,
            let libraryName,
            let fingerprint
        ):
            UnlockView(
                libraryId: libraryId,
                libraryName: libraryName,
                fingerprint: fingerprint,
                onUnlocked: {
                    appDelegate.openLocalLibrary(
                        id: libraryId,
                    )
                },
                // Cancelling a switch-to-locked-library returns to the
                // library that's still open.
                onCancel: { appDelegate.screen = .library },
            )
        case .library:
            MainAppView()
        }
    }

    /// Modal hosts attached to the main window: the welcome flow (New
    /// Library / Restore from Code), the rename sheet, and the lock
    /// confirmation. Each is driven by `AppDelegate` trigger state set from
    /// the File menu and acts on the active library via `appService.sync`.
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
                welcomeView
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
                    },
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
            appDelegate.appService?.configStore.config.libraryName
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
            welcomeView
        case .unlock(
            let libraryId,
            let libraryName,
            let fingerprint
        ):
            UnlockView(
                libraryId: libraryId,
                libraryName: libraryName,
                fingerprint: fingerprint,
                onUnlocked: {
                    appDelegate.openLocalLibrary(
                        id: libraryId,
                    )
                },
                // No shell yet (first launch, or opening from the welcome
                // chooser): cancelling returns to the welcome.
                onCancel: { appDelegate.screen = .welcome },
            )
        case .library:
            // Shouldn't happen: hasShell becomes true the moment we
            // reach .library. Fall through to MainAppView defensively.
            MainAppView()
        }
    }

    var body: some Scene {
        mainWindow
        storageManagerWindow
        settingsWindow
    }
}

extension BaeApp {
    private var mainWindow: some Scene {
        Window("bae", id: "main") {
            libraryModals(
                Group {
                    if appDelegate.hasShell {
                        detailContent
                    }
                    else {
                        // Bootstrap screens lay out as a vertical stack (the
                        // loading case centers a spinner between two Spacers).
                        VStack(spacing: 0) {
                            bootstrapContent
                        }
                    }
                }
                .frame(minWidth: 900, minHeight: 600)
                .windowBackground()
                .navigationTitle(windowTitle)
                .overlay(alignment: .bottom) {
                    if let loadError = appDelegate.loadError {
                        Text(loadError)
                            .foregroundStyle(.red)
                            .padding()
                    }
                }
            )
            .environment(appDelegate.appService?.playbackStore)
            .environment(appDelegate.appService?.configStore)
            .environment(appDelegate.appService?.importStore)
            .environment(appDelegate.appService?.libraryStore)
            .environment(appDelegate.appService?.projectionRegistry)
            .environment(appDelegate.appService?.mediaPaths)
            .environment(appDelegate.appService?.playback)
            .environment(appDelegate.appService?.queue)
            .environment(appDelegate.appService?.previewAudio)
            .environment(appDelegate.appService?.library)
            .environment(appDelegate.appService?.releaseEditor)
            .environment(appDelegate.appService?.importer)
            .environment(appDelegate.appService?.sync)
            .environment(appDelegate.appService?.downloads)
            .environment(appDelegate.appService?.exports)
            .environment(appDelegate.appService?.discogs)
            .environment(appDelegate.appService?.automation)
            .environment(appDelegate.appService?.export)
            .environment(appDelegate.appService?.outboxStore)
            .environment(appDelegate.appService?.downloadStore)
            .environment(appDelegate.appService?.exportStore)
            .environment(\.playbackPositionPublisher, playbackPublisher)
            .environment(\.previewProgressPublisher, previewPublisher)
            .environment(\.importLoudnessPublisher, importLoudnessPublisher)
            .environment(appDelegate.uiStore)
            .onAppear {
                #if !DEBUG
                    updaterController.updater.checkForUpdatesInBackground()
                #endif
            }
        }
        .windowStyle(.hiddenTitleBar)
        .commandsRemoved()
    }

    private var storageManagerWindow: some Scene {
        Window("Storage Manager", id: "storage-manager") {
            if let appService = appDelegate.appService {
                StorageManagerView()
                    .environment(appService.libraryStore)
                    .environment(appService.projectionRegistry)
                    .environment(appService.mediaPaths)
                    .environment(appService.releaseEditor)
                    .environment(appService.library)
                    .environment(appService.outboxStore)
                    .environment(appService.downloadStore)
                    .environment(appService.exportStore)
                    .environment(appService.sync)
                    .environment(appService.downloads)
                    .environment(appService.exports)
                    .environment(appService.configStore)
                    .environment(appDelegate.uiStore)
                    .environment(\.playbackPositionPublisher, playbackPublisher)
                    .environment(\.previewProgressPublisher, previewPublisher)
            }
            else {
                ContentUnavailableView(
                    "No library loaded",
                    systemImage: "internaldrive",
                    description: Text("Open a library first"),
                )
                .frame(width: 300, height: 200)
            }
        }
        .defaultSize(width: 800, height: 500)
        .commandsRemoved()
    }

    private var settingsWindow: some Scene {
        Settings {
            if let appService = appDelegate.appService {
                SettingsView(checkForUpdatesViewModel: checkForUpdatesViewModel)
                    .environment(appService.configStore)
                    .environment(appService.libraryStore)
                    .environment(appService.playback)
                    .environment(appService.sync)
                    .environment(appService.exports)
                    .environment(appService.discogs)
                    .environment(appService.automation)
                    .environment(\.playbackPositionPublisher, playbackPublisher)
                    .environment(\.previewProgressPublisher, previewPublisher)
                    .environment(appDelegate.uiStore)
                    .errorAlert(appDelegate.uiStore)
            }
            else {
                ContentUnavailableView(
                    "No library loaded",
                    systemImage: "books.vertical",
                    description: Text(
                        "Open a library first to access settings"
                    ),
                )
                .frame(width: 300, height: 200)
            }
        }
        .commands {
            CommandGroup(after: .appInfo) {
                CheckForUpdatesView(viewModel: checkForUpdatesViewModel)
            }
            if let appService = appDelegate.appService {
                MainAppMenuCommands(
                    playback: appService.playback,
                    importer: appService.importer,
                    libraryStore: appService.libraryStore,
                    playbackStore: appService.playbackStore,
                    uiStore: appDelegate.uiStore,
                    libraries: appDelegate.libraries,
                    onNewLibrary: { mode in
                        appDelegate.welcomeInitialMode = mode
                        appDelegate.showAddLibrarySheet = true
                    },
                    onOpenLibrary: { appDelegate.openLibrary($0) },
                    onSwitchOffset: { appDelegate.switchLibrary(byOffset: $0) },
                    onRenameLibrary: {
                        let cfg = appService.configStore.config
                        appDelegate.renameLibrarySheet =
                            RenameLibrarySheetState(
                                id: cfg.libraryId,
                                newName: cfg.libraryName
                            )
                    },
                    onLockLibrary: { appDelegate.confirmLockLibrary = true },
                    onSyncNow: { appService.sync.triggerSync() },
                    onRevealLibrary: {
                        SystemActions.revealInFinder(
                            path: appService.configStore.config.libraryPath
                        )
                    },
                    onCopyLibraryId: {
                        SystemActions.copyToPasteboard(
                            appService.configStore.config.libraryId
                        )
                    },
                    onCloseLibrary: { appDelegate.closeLibrary() }
                )
            }
        }
    }
}

// MARK: - AppDelegate

@Observable
final class AppDelegate: NSObject, NSApplicationDelegate, @unchecked Sendable {
    var appService: AppService?
    private let mediaControlService = MediaControlService()
    var uiStore = UiStore()
    var screen: AppScreen = .loading
    var loadError: String?
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
    /// The in-flight library open, owned so a superseding open (a fast switch)
    /// can cancel it before it lands a stale library on `screen`/`appService`.
    @ObservationIgnored
    private var openTask: Task<Void, Never>?
    /// In-flight library-list reload, cancelled when a newer one supersedes it.
    @ObservationIgnored
    private var reloadTask: Task<Void, Never>?
    /// In-flight rename / lock, cancelled on library close.
    @ObservationIgnored
    private var renameTask: Task<Void, Never>?
    @ObservationIgnored
    private var lockTask: Task<Void, Never>?

    private var skipsApplicationServices: Bool {
        AppRuntime.skipsApplicationServices(
            environment: appProcessEnvironment
        )
    }

    func applicationDidFinishLaunching(_: Notification) {
        if !skipsApplicationServices {
            BaeCrashReporting.configure()
            BaeDiagnostics.configure(source: "macos")
            logger.info("application launched")
            initKeyring()
            // Hand Rust the CloudKit driver once. It can't build the driver
            // itself (it needs the platform CloudKit APIs); installing it is
            // idempotent and harmless for libraries that sync elsewhere, so it
            // belongs here at the composition root rather than at each open.
            #if BAE_CLOUDKIT
                setCloudkitDriver(driver: CloudKitService.bae())
            #endif
        }
        loadInitialState()
    }

    // MARK: - Library lifecycle

    private func loadInitialState() {
        if skipsApplicationServices {
            screen = .welcome
            return
        }
        do {
            let libraries = try discoverLibraries()
            self.libraries = libraries
            if libraries.isEmpty {
                screen = .welcome
                return
            }
            openLibrary(libraries[0])
        }
        catch {
            loadError = error.localizedDescription
        }
    }

    func openLibrary(_ library: BridgeLibrary) {
        openLocalLibrary(id: library.id)
    }

    func openLocalLibrary(id libraryId: String) {
        loadError = nil
        screen = .loading
        let store = uiStore
        // Cancel any open still in flight: a fast library switch starts a new
        // open before the previous `initApp` returns, and the superseded one
        // must not land its (now stale) library on `screen`/`appService`.
        openTask?.cancel()
        openTask = Task { @MainActor in
            do {
                let handle = try await openHandle(libraryId: libraryId)
                // A newer open may have superseded this one while `initApp`
                // ran; bail before touching screen/appService so the stale open
                // can't clobber the current one. `handle` drops here.
                try Task.checkCancellation()
                let cfg = handle.getConfig()
                if needsUnlock(handle: handle, config: cfg) {
                    screen = .unlock(
                        libraryId: libraryId,
                        libraryName: cfg.libraryName,
                        fingerprint: cfg.encryptionKeyFingerprint,
                    )
                    return
                }
                let initialOutbox: BridgeOutboxSnapshot
                do {
                    initialOutbox = try await handle.getOutboxSnapshot()
                }
                catch {
                    await failOpenBeforeService(handle: handle, error: error)
                    return
                }
                let service = makeService(
                    handle: handle,
                    uiStore: store,
                    config: cfg,
                    initialOutbox: initialOutbox
                )
                if handle.isSyncReady() {
                    service.sync.storeRestoreCodeInKeychain(
                        libraryId: cfg.libraryId,
                        onError: { [store] in store.showError($0) }
                    )
                }
                self.appService = service
                self.screen = .library
                self.reloadLibraries()
                self.hasShell = true
                self.showAddLibrarySheet = false
            }
            catch is CancellationError {
                // Superseded by a newer open (or a close); that call owns
                // screen/appService.
                logger.debug(
                    "Library open superseded before it could land; skipping"
                )
            }
            catch {
                self.loadError = error.localizedDescription
                // A bootstrap open (first launch, or reopening from the
                // welcome chooser after a close) that fails must return to
                // the welcome so the user can retry or pick another, rather
                // than strand on the loading spinner. A switch failure keeps
                // the shell mounted and its own state.
                if !self.hasShell {
                    self.screen = .welcome
                }
            }
        }
    }

    private func openHandle(libraryId: String) async throws -> AppHandle {
        try await DetachedWork.run {
            try initApp(
                libraryId: libraryId,
                positionUpdateIntervalMs: 200,
            )
        }
    }

    private func needsUnlock(handle: AppHandle, config: BridgeConfig) -> Bool {
        config.encryptionKeyStored && !handle.hasEncryptionKey()
    }

    private func failOpenBeforeService(handle: AppHandle, error: Error) async {
        logger.error("Failed to seed outbox snapshot: \(error)")
        loadError = error.localizedDescription
        screen = .welcome
        await handle.shutdown()
    }

    @MainActor
    private func makeService(
        handle: AppHandle,
        uiStore: UiStore,
        config: BridgeConfig,
        initialOutbox: BridgeOutboxSnapshot
    ) -> AppService {
        let service = AppService(
            appHandle: handle,
            mediaControlService: mediaControlService,
            uiStore: uiStore,
            config: config,
            initialOutbox: initialOutbox
        )
        service.wireUp()
        return service
    }

    /// Close the open library and return to the welcome chooser. Stops
    /// playback, drops the `AppService` — which releases the `AppHandle` and
    /// ends the core's UI-event subscription as the broadcast sender drops —
    /// and replaces `uiStore` so the next library opens with fresh navigation
    /// state. Flipping `hasShell` back to false routes the window from the
    /// main content back to the bootstrap `WelcomeView`.
    @MainActor
    func closeLibrary() {
        guard let service = appService else { return }
        // Cancel any open still in flight so a parked `initApp` can't resume past
        // its post-await cancellation check and write `.library`/`appService`
        // back after the close.
        openTask?.cancel()
        renameTask?.cancel()
        lockTask?.cancel()
        mediaControlService.deactivate(playbackStore: service.playbackStore)
        Task { [handle = service.appHandle] in
            await handle.shutdown()
        }
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
        reloadTask?.cancel()
        reloadTask = Task { @MainActor in
            do {
                libraries = try await DetachedWork.run {
                    try discoverLibraries()
                }
            }
            catch is CancellationError {
                logger.debug("discoverLibraries cancelled")
            }
            catch {
                logger.error(
                    "Failed to list libraries: \(error.localizedDescription)"
                )
            }
        }
    }

    /// Switch to the library `offset` positions from the active one in the
    /// discovered list, wrapping around the ends. No-op with one (or no)
    /// library or no active library.
    func switchLibrary(byOffset offset: Int) {
        guard libraries.count > 1,
            let activeIdx = libraries.firstIndex(where: \.isActive)
        else {
            return
        }
        let count = libraries.count
        let next = ((activeIdx + offset) % count + count) % count
        openLibrary(libraries[next])
    }

    /// Rename a library off the main actor, then refresh the list (and the
    /// window title, which reads the active library's name). On failure the
    /// error is written back into the open sheet; the sheet stays up so the
    /// user can retry.
    func renameLibrary(_ libraryId: String, to newName: String) {
        guard let sync = appService?.sync else { return }
        let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        renameTask?.cancel()
        renameTask = Task { @MainActor in
            do {
                try await DetachedWork.run {
                    try sync.renameLibrary(libraryId, trimmed)
                }
                renameLibrarySheet = nil
                reloadLibraries()
            }
            catch is CancellationError {
                logger.debug("rename cancelled for \(libraryId)")
            }
            catch {
                logger.error(
                    "Failed to rename \(libraryId): \(error.localizedDescription)"
                )
                renameLibrarySheet?.error = error.localizedDescription
            }
        }
    }

    /// Lock the active library off the main actor: drop its encryption key from
    /// the keychain. The current session keeps working; the key is needed again
    /// on next launch. Errors surface through `loadError`.
    func lockActiveLibrary() {
        guard let sync = appService?.sync else { return }
        lockTask?.cancel()
        lockTask = Task { @MainActor in
            do {
                try await DetachedWork.run {
                    try sync.lockActiveLibrary()
                }
            }
            catch is CancellationError {
                logger.debug("lock cancelled")
            }
            catch {
                logger.error(
                    "Failed to lock library: \(error.localizedDescription)"
                )
                loadError = error.localizedDescription
            }
        }
    }
}

extension AppDelegate {
    func applicationWillTerminate(_: Notification) {
        guard UserDefaults.standard.bool(forKey: "persistPlayback"),
            let appService
        else {
            return
        }
        Task { [handle = appService.appHandle] in
            await handle.shutdown()
        }
    }

    func applicationDidBecomeActive(_: Notification) {
        // Refresh the library list when bae returns to the foreground so the
        // Open Library submenu reflects libraries created, renamed, or removed
        // elsewhere while we were in the background. Only meaningful once a
        // library is open — the menu that consumes the list exists only then.
        guard !skipsApplicationServices, appService != nil else {
            return
        }
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
        do {
            try appService?.appHandle.addWatchedFolder(path: url.path)
        }
        catch {
            uiStore.showError(
                String(
                    localized:
                        "Couldn't add folder: \(error.localizedDescription)"
                )
            )
        }
        uiStore.navigateToImport()
    }
}
