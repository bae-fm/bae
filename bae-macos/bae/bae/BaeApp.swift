import BaeKit
import Sparkle
import SwiftUI
import os.log

let baeAppLogger = Logger.bae("BaeApp")
let baeAppProcessEnvironment = ProcessInfo.processInfo.environment
let baeAppEdition: AppEdition = {
    #if BAE_OAUTH_PROVIDERS
        .bae
    #else
        .baeium
    #endif
}()

enum AppRuntime: Equatable {
    case application
    case preview
    case testHost

    init(environment: [String: String]) {
        if Self.isPreview(environment: environment) {
            self = .preview
        }
        else if Self.isTestHost(environment: environment) {
            self = .testHost
        }
        else {
            self = .application
        }
    }

    var startsApplicationServices: Bool {
        self == .application
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

        /// The folder a UI test has the opened library watch.
        static func watchedFolderForUITesting(
            environment: [String: String]
        ) -> String? {
            environment["BAE_UI_TESTING_WATCH_FOLDER"]
        }
    #endif
}

private let appRuntime = AppRuntime(environment: baeAppProcessEnvironment)

private func discoverInitialLibraries(
    host: BridgeHost,
    environment: [String: String]
) throws -> [BridgeLibrary] {
    var libraries = try host.discoverLibraries()
    #if DEBUG
        if libraries.isEmpty,
            AppRuntime.createsLibraryForUITesting(environment: environment)
        {
            _ = try host.createLibrary(name: nil)
            libraries = try host.discoverLibraries()
        }
    #endif
    return libraries
}

/// bae's directory under the launch environment's `HOME`, which a UI test
/// sets to get a fresh one.
private func baeAppDir(environment: [String: String]) -> BridgeAppDir {
    guard let home = environment["HOME"], !home.isEmpty else {
        preconditionFailure("HOME is unset, so bae's directory has no location")
    }
    return BridgeAppDir(home: home)
}

enum AppScreen {
    case loading
    case welcome
    case unlock(libraryName: String)
    case keychainLocked(libraryId: String)
    case library
}

/// The installed application's process-wide services, built together so an
/// inert preview or test host starts none of them.
@MainActor
final class ApplicationServices {
    let diagnostics: BridgeDiagnostics
    let host: BridgeHost
    let mediaControlService: MediaControlService
    let librarySetup: LibrarySetup
    let updaterController: SPUStandardUpdaterController
    let checkForUpdatesViewModel: CheckForUpdatesViewModel
    let firstResponderActions = FirstResponderActions()

    init() {
        let appDir = baeAppDir(environment: baeAppProcessEnvironment)
        diagnostics = BaeDiagnostics.configure(
            source: "macos",
            edition: baeAppEdition
        )
        host = BaeHost.make(diagnostics: diagnostics, appDir: appDir)
        mediaControlService = MediaControlService()
        librarySetup = LibrarySetup.live(host: host)
        #if DEBUG
            let updaterController = SPUStandardUpdaterController(
                startingUpdater: false,
                updaterDelegate: nil,
                userDriverDelegate: nil
            )
        #else
            let updaterController = SPUStandardUpdaterController(
                startingUpdater: true,
                updaterDelegate: nil,
                userDriverDelegate: nil
            )
        #endif
        self.updaterController = updaterController
        checkForUpdatesViewModel = CheckForUpdatesViewModel(
            updater: updaterController.updater
        )
    }
}

@main
struct BaeApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self)
    var appDelegate
    private var applicationServices: ApplicationServices {
        appDelegate.requiredApplicationServices
    }

    /// The active library's name, else "bae".
    private var windowTitle: String {
        if let name = appDelegate.appService?.libraryName, !name.isEmpty {
            return String(localized: "\(name) - bae")
        }
        return "bae"
    }

    /// The welcome flow in the mode a menu item asked for, showing
    /// `loadError` (nil in the Add Library sheet, whose failures the shell
    /// shows).
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
        .environment(applicationServices.librarySetup)
    }

    /// Main-window content once the first library has opened.
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

    /// The welcome, rename and lock modals the File menu presents.
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
                    // The dismissal frame shows the last real value.
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

    /// The lock confirmation's body, naming the library.
    private var lockConfirmMessage: String {
        let name =
            appDelegate.appService?.libraryName
            ?? String(localized: "This library")
        return String(
            localized:
                "\(name)'s encryption key will be removed from the keychain. This session keeps working; you'll need to re-enter the key on next launch."
        )
    }

    /// Content shown before any library has opened.
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
            // Bootstrap content stays independent of the service environment.
            Spacer()
            ProgressView()
            Spacer()
        }
    }

    /// Loading takes the library window's size; everything else the welcome
    /// size.
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
        if let applicationServices = appDelegate.applicationServices {
            CommandGroup(after: .appInfo) {
                CheckForUpdatesView(
                    viewModel: applicationServices.checkForUpdatesViewModel
                )
            }
            LibraryFileMenuCommands(
                libraries: appDelegate.libraries,
                onNewLibrary: { appDelegate.presentWelcome(mode: $0) },
                onOpenLibrary: { appDelegate.openLibrary($0) },
                onSwitchOffset: { appDelegate.switchLibrary(byOffset: $0) },
                onRenameLibrary: { appDelegate.presentRenameLibrary() },
                onLockLibrary: {
                    appDelegate.presentLockLibraryConfirmation()
                },
                onSyncNow: { appDelegate.syncNow() },
                onRevealLibrary: { appDelegate.revealLibraryInFinder() },
                onCopyLibraryId: { appDelegate.copyLibraryId() },
                onCloseLibrary: { appDelegate.closeLibrary() }
            )
            MainAppMenuCommands(
                responderActions: applicationServices.firstResponderActions
            )
        }
    }

    /// One primary WindowGroup changes content and content-driven size as the
    /// library opens and closes; the window itself is never replaced.
    private var primaryWindow: some Scene {
        WindowGroup("bae", id: MainWindow.sceneID) {
            Group {
                if !appDelegate.runtime.startsApplicationServices {
                    EmptyView()
                }
                else if appDelegate.hasShell,
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
            .appAppearance()
            .navigationTitle(appDelegate.hasShell ? windowTitle : "bae")
            .onAppear {
                #if !DEBUG
                    appDelegate.applicationServices?.updaterController.updater
                        .checkForUpdatesInBackground()
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
            if appDelegate.runtime.startsApplicationServices {
                StorageManagerWindowRoot(appDelegate: appDelegate)
                    .appAppearance()
            }
            else {
                EmptyView()
            }
        }
        .defaultSize(width: 800, height: 500)
        // A restored auxiliary window suppresses the primary window's launch.
        .restorationBehavior(.disabled)
    }

    /// A `View` so Observation tracks its `appService` read and it re-renders
    /// once the library opens.
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
                .frame(
                    width: NoLibraryPlaceholder.width,
                    height: NoLibraryPlaceholder.height
                )
            }
        }
    }

    private var settingsWindow: some Scene {
        Settings {
            SettingsWindowRoot(
                appDelegate: appDelegate,
                checkForUpdatesViewModel:
                    appDelegate.applicationServices?.checkForUpdatesViewModel
            )
            .appAppearance()
        }
        // A restored auxiliary window suppresses the primary window's launch.
        .restorationBehavior(.disabled)
    }

    /// A `View` so Observation tracks its `appService` read and it re-renders
    /// once the library opens.
    private struct SettingsWindowRoot: View {
        let appDelegate: AppDelegate
        let checkForUpdatesViewModel: CheckForUpdatesViewModel?

        var body: some View {
            if !appDelegate.runtime.startsApplicationServices {
                EmptyView()
            }
            else if let appService = appDelegate.appService,
                let checkForUpdatesViewModel
            {
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
                .frame(
                    width: NoLibraryPlaceholder.width,
                    height: NoLibraryPlaceholder.height
                )
            }
        }
    }
}

/// A window's "No library loaded" placeholder.
private enum NoLibraryPlaceholder {
    static let width: CGFloat = 300
    static let height: CGFloat = 200
}

// MARK: - AppDelegate

@MainActor
@Observable
final class AppDelegate: NSObject, NSApplicationDelegate {
    let runtime: AppRuntime
    private(set) var applicationServices: ApplicationServices?
    var appService: AppService?
    var requiredApplicationServices: ApplicationServices {
        guard let applicationServices else {
            preconditionFailure(
                "Application services are unavailable in the inert app host"
            )
        }
        return applicationServices
    }
    var uiStore = UiStore()
    var screen: AppScreen = .loading
    var loadError: DisplayError?
    /// The welcome mode a menu item asked for; nil is the chooser.
    var welcomeInitialMode: WelcomeView.Mode?
    /// True once the first library has opened and replaced the bootstrap
    /// screens.
    var hasShell: Bool = false
    /// Presents the welcome sheet over an open library.
    var showAddLibrarySheet: Bool = false
    /// Every library on this device, for the File → Open Library submenu.
    var libraries: [BridgeLibrary] = []
    /// The Rename Library sheet's state while it is open.
    var renameLibrarySheet: RenameLibrarySheetState?
    /// Drives the Lock Library confirmation alert.
    var confirmLockLibrary: Bool = false
    /// The shared open sequence; each open ends in an `Outcome` this delegate
    /// applies.
    @ObservationIgnored
    private lazy var opener = LibrarySessionOpener<AppHandle, AppService>(
        // Captured so the `@Sendable` closure reads no main-actor state.
        makeHandle: {
            [host = requiredApplicationServices.host] libraryId in
            try initApp(
                libraryId: libraryId,
                positionUpdateIntervalMs: 200,
                // The "Restore on launch" preference.
                restorePlayback: UserDefaults.standard.bool(
                    forKey: "persistPlayback"
                ),
                // Telemetry is up before the library opens.
                host: host
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
    /// The one graceful shutdown that Close and Quit share.
    @ObservationIgnored
    private let shutdownCoordinator =
        LibraryShutdownCoordinator<AppService>()

    override convenience init() {
        self.init(runtime: appRuntime)
    }

    init(
        runtime: AppRuntime,
        makeApplicationServices: () -> ApplicationServices = {
            ApplicationServices()
        }
    ) {
        self.runtime = runtime
        applicationServices =
            runtime.startsApplicationServices
            ? makeApplicationServices()
            : nil
        super.init()
    }
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

    /// Discover this Mac's libraries and land on a screen, auto-opening one
    /// only when `canOpenLibraries`.
    func loadInitialState(canOpenLibraries: Bool) {
        do {
            let libraries = try discoverInitialLibraries(
                host: requiredApplicationServices.host,
                environment: baeAppProcessEnvironment
            )
            self.libraries = libraries
            // Auto-open only a library whose config loaded.
            guard canOpenLibraries,
                let openable = libraries.first(where: { $0.error == nil })
            else {
                screen = .welcome
                return
            }
            openLibrary(openable)
        }
        catch {
            // The welcome screen shows `loadError`.
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
                // The newer open or close owns the screen.
                baeAppLogger.debug(
                    "Library open superseded before it could land; skipping"
                )
            case .failed(let error):
                self.loadError = DisplayError(error)
                // A failed bootstrap open returns to the welcome.
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
        #if DEBUG
            if let folder = AppRuntime.watchedFolderForUITesting(
                environment: baeAppProcessEnvironment
            ) {
                service.watchFolderForUITesting(folder)
            }
        #endif
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
            mediaControlService:
                requiredApplicationServices.mediaControlService,
            diagnostics: requiredApplicationServices.diagnostics,
            uiStore: uiStore,
            config: config,
            initialOutbox: initialOutbox
        )
        service.wireUp()
        return service
    }

    /// Close the open library and return to the welcome once it has shut
    /// down.
    func closeLibrary() {
        guard let service = appService else { return }
        prepareForLibraryShutdown()
        screen = .loading
        beginLibraryShutdown(service)
    }

    @discardableResult
    func beginLibraryShutdown(_ service: AppService)
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
            baeAppLogger.error(
                "Failed to shut down library: \(failure.diagnostic)"
            )
            loadError = failure.displayedError
            screen = .library
        }
    }

    func prepareForLibraryShutdown() {
        // An open still in flight must not replace this session.
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

    /// Reload the library list, keeping the last good one on failure.
    func reloadLibraries() {
        let host = requiredApplicationServices.host
        reloadSlot.replace(
            "discoverLibraries",
            work: { try host.discoverLibraries() },
            onSuccess: { self.libraries = $0 },
            onError: {
                baeAppLogger.error(
                    "Failed to list libraries: \($0.localizedDescription)"
                )
            }
        )
    }

    /// Open the library `offset` places from the active one, wrapping and
    /// skipping broken ones.
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

    /// Rename a library, showing a failure in the open sheet.
    func renameLibrary(_ libraryId: String, to newName: String) {
        guard let appService else { return }
        let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        renameSlot.replace(
            "rename of \(libraryId)",
            work: {
                try await appService.renameLibrary(libraryId, to: trimmed)
            },
            onSuccess: {
                self.renameLibrarySheet = nil
                self.reloadLibraries()
            },
            onError: {
                baeAppLogger.error(
                    "Failed to rename \(libraryId): \($0.localizedDescription)"
                )
                self.renameLibrarySheet?.error = $0.localizedDescription
            }
        )
    }

    /// Remove the active library's key from the keychain.
    func lockActiveLibrary() {
        guard let appService else { return }
        lockSlot.replace(
            "lock",
            work: { try await appService.lockActiveLibrary() },
            onSuccess: {},
            onError: {
                baeAppLogger.error(
                    "Failed to lock library: \($0.localizedDescription)"
                )
                self.loadError = DisplayError($0)
            }
        )
    }
}

extension AppDelegate {
    /// Close the active library, then remove it from this device.
    func forgetActiveLibrary() {
        guard let service = appService else {
            baeAppLogger.warning(
                "Ignoring remove-library request: no library is open."
            )
            return
        }
        let libraryId = service.libraryId
        forgetSlot.replace(
            "close for removal",
            work: { try service.closeLibrary() },
            onSuccess: {
                self.prepareForLibraryShutdown()
                self.releaseLibrarySession(service)
                self.removeClosedLibrary(libraryId)
            },
            onError: {
                baeAppLogger.error(
                    "Failed to close the library for removal: \($0.localizedDescription)"
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

    /// Remove a closed library once nothing holds its store.
    private func removeClosedLibrary(_ libraryId: String) {
        let host = requiredApplicationServices.host
        forgetSlot.replace(
            "remove \(libraryId)",
            work: { try host.removeLocalLibrary(libraryId: libraryId) },
            onSuccess: { self.reloadLibraries() },
            onError: {
                baeAppLogger.error(
                    "Failed to remove library: \($0.localizedDescription)"
                )
                self.loadError = DisplayError($0)?
                    .addingContext(
                        String(localized: "Couldn't remove library")
                    )
                self.reloadLibraries()
            }
        )
    }
}
