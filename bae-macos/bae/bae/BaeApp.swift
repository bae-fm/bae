import BaeKit
import Sparkle
import SwiftUI
import Synchronization
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

private let appRuntime = AppRuntime(environment: baeAppProcessEnvironment)

#if DEBUG
    /// The fixture a UI test names, which the first library this process opens
    /// is written from: a later open finds the library already holding it.
    final class UITestFixture: Sendable {
        private let path: Mutex<String?>

        init(path: String?) {
            self.path = Mutex(path)
        }

        /// The fixture's path the first time it is asked for, then `nil`.
        func take() -> String? {
            path.withLock { path in
                defer { path = nil }
                return path
            }
        }
    }
#endif

private func discoverInitialLibraries(
    host: BridgeHost,
    environment: [String: String]
) async throws -> [BridgeLibrary] {
    var libraries = try await host.listLibraries()
    #if DEBUG
        if libraries.isEmpty,
            AppRuntime.createsLibraryForUITesting(environment: environment)
        {
            // Opened and closed so the fixture can be written into it when the
            // launch opens it by id.
            let created = try await host.createLibrary(name: nil)
            let app = try created.open(
                positionUpdateIntervalMs: 200,
                restorePlayback: false
            )
            try app.closeLibrary()
            libraries = try await host.listLibraries()
        }
    #endif
    return libraries
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
                    onLibraryReady: appDelegate.openOpenedLibrary,
                    onOpenLibrary: appDelegate.openLocalLibrary(id:),
                    initialMode: mode,
                    canDeleteActiveLibrary: !appDelegate.hasShell
                )
            }
            else {
                WelcomeView(
                    onLibraryReady: appDelegate.openOpenedLibrary,
                    onOpenLibrary: appDelegate.openLocalLibrary(id:),
                    loadError: loadError,
                    canDeleteActiveLibrary: !appDelegate.hasShell
                )
            }
        }
        .environment(applicationServices.librarySetup)
        .environment(applicationServices.libraryRemovals)
    }

    /// Main-window content once the first library has opened.
    @ViewBuilder
    private var detailContent: some View {
        switch appDelegate.screen {
        case .loading:
            ProgressView("Switching libraries...")
        case .welcome:
            ProgressView()
        case .resetRequired(let libraryId):
            deviceResetView(libraryId: libraryId)
        case .restoreRequired(let libraryId):
            deviceRestoreView(libraryId: libraryId)
        case .keychainLocked:
            KeychainLockedView(onRetry: appDelegate.retryKeychainOpen)
        case .library:
            MainAppView()
        }
    }

    private func deviceResetView(libraryId: String) -> some View {
        DeviceResetView(
            error: appDelegate.loadError,
            onReset: { appDelegate.resetDevice(libraryId: libraryId) },
            onCancel: appDelegate.cancelDeviceReset
        )
    }

    /// The person's current restore code, to restore a library over this
    /// device's copy; going back opens the library again as it was.
    private func deviceRestoreView(libraryId: String) -> some View {
        BootstrapCodeView(
            form: applicationServices.librarySetup.bootstrapForm(
                entry: .restoreOver(libraryId: libraryId),
                onLibraryReady: appDelegate.openOpenedLibrary
            ),
            onBack: { appDelegate.openLocalLibrary(id: libraryId) }
        )
        .id(libraryId)
    }

    /// The welcome and rename modals the File menu presents.
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
        case .resetRequired(let libraryId):
            deviceResetView(libraryId: libraryId)
        case .restoreRequired(let libraryId):
            deviceRestoreView(libraryId: libraryId)
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
        if let menuBar = appDelegate.menuBar {
            MainAppMenuCommands(menuBar: menuBar, app: appDelegate)
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
                        onForgetLibrary: { appDelegate.forgetActiveLibrary() },
                        onResetDevice: {
                            appDelegate.presentDeviceReset(
                                libraryId: appService.libraryId
                            )
                        },
                        onRestoreFromCode: {
                            appDelegate.presentDeviceRestore(
                                libraryId: appService.libraryId
                            )
                        }
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
    /// What the main menu shows, read from this delegate and its services;
    /// present whenever the services are.
    @ObservationIgnored
    private(set) var menuBar: MenuBar?
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
    /// The shared open sequence; each open ends in an `Outcome` this delegate
    /// applies.
    @ObservationIgnored
    private lazy var opener: LibrarySessionOpener<AppHandle, AppService> = {
        // Captured so the `@Sendable` closure reads no main-actor state.
        let host = requiredApplicationServices.host
        #if DEBUG
            let fixture = UITestFixture(
                path: AppRuntime.libraryFixtureForUITesting(
                    environment: baeAppProcessEnvironment
                )
            )
        #endif
        return LibrarySessionOpener<AppHandle, AppService>(
            makeHandle: { libraryId in
                // The "Restore on launch" preference.
                let restorePlayback = UserDefaults.standard.bool(
                    forKey: "persistPlayback"
                )
                #if DEBUG
                    if let fixturePath = fixture.take() {
                        return try initAppOnFixture(
                            libraryId: libraryId,
                            positionUpdateIntervalMs: 200,
                            restorePlayback: restorePlayback,
                            fixturePath: fixturePath,
                            host: host
                        )
                    }
                #endif
                return try initApp(
                    libraryId: libraryId,
                    positionUpdateIntervalMs: 200,
                    restorePlayback: restorePlayback,
                    // Telemetry is up before the library opens.
                    host: host
                )
            },
            makeService: { [weak self] handle, config, seed in
                guard let self else {
                    preconditionFailure("AppDelegate outlives its opener")
                }
                return self.makeService(
                    handle: handle,
                    uiStore: self.uiStore,
                    config: config,
                    seed: seed
                )
            }
        )
    }()
    /// In-flight library-list reload, cancelled when a newer one supersedes it.
    private let reloadSlot = CancellableTaskSlot()
    /// In-flight rename, cancelled on library close.
    private let renameSlot = CancellableTaskSlot()
    /// In-flight forget, cancelled on library close.
    private let forgetSlot = CancellableTaskSlot()
    /// In-flight device reset.
    private let resetSlot = CancellableTaskSlot()
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
        if let applicationServices {
            menuBar = MenuBar { [unowned self] in
                self.readMenuBar(applicationServices)
            }
        }
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
        let host = requiredApplicationServices.host
        Task {
            do {
                libraries = try await discoverInitialLibraries(
                    host: host,
                    environment: baeAppProcessEnvironment
                )
            }
            catch {
                // The welcome screen shows `loadError`.
                baeAppLogger.error(
                    "Failed to list libraries: \(error.localizedDescription)"
                )
                loadError = DisplayError(error)
                screen = .welcome
                return
            }
            // Core lists the active library first; one whose settings can't
            // be read shows on the welcome, saying why.
            guard canOpenLibraries, let library = libraries.first,
                library.isOpenable
            else {
                screen = .welcome
                return
            }
            openLibrary(library)
        }
    }

    func openLibrary(_ library: BridgeLibrary) {
        openLocalLibrary(id: library.id)
    }

    func openLocalLibrary(id libraryId: String) {
        loadError = nil
        screen = .loading
        opener.open(libraryId: libraryId) { [weak self] outcome in
            self?.applyOpenOutcome(outcome, libraryId: libraryId, warning: nil)
        }
    }

    /// Open the store a create, restore, join or device reset returned,
    /// showing the error a partly finished restore or join reported once the
    /// library is up.
    func openOpenedLibrary(_ library: OpenedLibrary, warning: BridgeError?) {
        loadError = nil
        screen = .loading
        let libraryId = library.libraryId()
        opener.open(
            makeHandle: {
                try library.open(
                    positionUpdateIntervalMs: 200,
                    // The "Restore on launch" preference.
                    restorePlayback: UserDefaults.standard.bool(
                        forKey: "persistPlayback"
                    )
                )
            },
            onOutcome: { [weak self] outcome in
                self?
                    .applyOpenOutcome(
                        outcome,
                        libraryId: libraryId,
                        warning: warning
                    )
            }
        )
    }

    private func applyOpenOutcome(
        _ outcome: LibrarySessionOpener<AppHandle, AppService>.Outcome,
        libraryId: String,
        warning: BridgeError?
    ) {
        switch outcome {
        case .opened(let service):
            landOpenedService(service)
            if let warning {
                uiStore.showError(warning)
            }
        case .keychainLocked:
            deferOpenForLockedKeychain(libraryId: libraryId)
        case .superseded:
            // The newer open or close owns the screen.
            baeAppLogger.debug(
                "Library open superseded before it could land; skipping"
            )
        case .failed(let error):
            loadError = DisplayError(error)
            if case BridgeError.Diagnostic(.deviceResetRequired, _) = error {
                screen = .resetRequired(libraryId: libraryId)
            }
            else if !hasShell {
                // A failed bootstrap open returns to the welcome.
                screen = .welcome
            }
        }
    }

    /// Show the device-reset warning for a library, from Settings' pending
    /// list.
    func presentDeviceReset(libraryId: String) {
        loadError = nil
        screen = .resetRequired(libraryId: libraryId)
    }

    /// Close the library and ask for the person's current restore code, to
    /// restore it over this device's copy after another of their devices
    /// replaced their keys. Going back opens the library again as it was.
    func presentDeviceRestore(libraryId: String) {
        let open = appService.flatMap { $0.libraryId == libraryId ? $0 : nil }
        if open != nil {
            prepareForLibraryShutdown()
        }
        loadError = nil
        screen = .loading
        resetSlot.replace {
            do {
                if let open {
                    try await open.shutdown()
                    try await DetachedWork.run { try open.closeLibrary() }
                    self.releaseLibrarySession(open)
                }
                self.screen = .restoreRequired(libraryId: libraryId)
            }
            catch {
                baeAppLogger.error(
                    "Closing the library to restore it failed: \(error.localizedDescription)"
                )
                self.loadError = DisplayError(error)
                self.screen = .welcome
            }
        }
    }

    /// Leave the reset warning without resetting.
    func cancelDeviceReset() {
        loadError = nil
        screen = hasShell ? .library : .welcome
    }

    /// Reset this device for a library: close it if it is open, then load it
    /// from storage again under a fresh device id. The person saw the warning
    /// before choosing, so nothing is shown after.
    func resetDevice(libraryId: String) {
        let host = requiredApplicationServices.host
        let deviceName = requiredApplicationServices.deviceName
        let open = appService.flatMap { $0.libraryId == libraryId ? $0 : nil }
        if open != nil {
            prepareForLibraryShutdown()
        }
        loadError = nil
        screen = .loading
        resetSlot.replace {
            do {
                if let open {
                    try await open.shutdown()
                    try await DetachedWork.run { try open.closeLibrary() }
                    self.releaseLibrarySession(open)
                    self.screen = .loading
                }
                let reset = try await host.resetDevice(
                    libraryId: libraryId,
                    deviceName: deviceName
                )
                self.openOpenedLibrary(reset.library, warning: reset.warning)
            }
            catch {
                baeAppLogger.error(
                    "Device reset failed: \(error.localizedDescription)"
                )
                self.loadError = DisplayError(error)
                self.screen = .resetRequired(libraryId: libraryId)
            }
        }
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
        seed: LibrarySessionSeed
    ) -> AppService {
        let service = AppService(
            appHandle: handle,
            mediaControlService:
                requiredApplicationServices.mediaControlService,
            diagnostics: requiredApplicationServices.diagnostics,
            uiStore: uiStore,
            config: config,
            seed: seed,
            deviceName: requiredApplicationServices.deviceName
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
        forgetSlot.cancel()
    }

    private func releaseLibrarySession(_ service: AppService) {
        guard appService === service else { return }
        service.deactivateMediaControls()
        appService = nil
        uiStore = UiStore()
        welcomeInitialMode = nil
        renameLibrarySheet = nil
        loadError = nil
        screen = .welcome
        hasShell = false
    }

    /// Reload the library list, keeping the last good one on failure.
    func reloadLibraries() {
        let host = requiredApplicationServices.host
        reloadSlot.replace(
            "listLibraries",
            work: { try await host.listLibraries() },
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
        let openable = libraries.filter(\.isOpenable)
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
                try await appService.renameLibrary(to: trimmed)
            },
            onSuccess: {
                self.renameLibrarySheet = nil
                self.reloadLibraries()
            },
            onError: {
                baeAppLogger.error(
                    "Failed to rename \(libraryId): \($0.localizedDescription)"
                )
                self.renameLibrarySheet?.error = $0.displayLine
            }
        )
    }
}

extension AppDelegate {
    /// Remove the open library from this Mac: it leaves the store's members,
    /// then its local copy is deleted. A refused removal opens it again.
    func forgetActiveLibrary() {
        guard let service = appService else {
            baeAppLogger.warning(
                "Ignoring remove-library request: no library is open."
            )
            return
        }
        let libraryId = service.libraryId
        let removals = requiredApplicationServices.libraryRemovals
        prepareForLibraryShutdown()
        screen = .loading
        forgetSlot.replace {
            do {
                let removal = try await service.removeLibrary(unsent: .discard)
                self.releaseLibrarySession(service)
                removals.record(removal)
                self.reloadLibraries()
            }
            catch {
                baeAppLogger.error(
                    "Failed to remove library: \(error.localizedDescription)"
                )
                // Removal stopped the library's services before leaving the
                // store failed; close what is left and open it again.
                do {
                    try await DetachedWork.run { try service.closeLibrary() }
                }
                catch let closeError {
                    baeAppLogger.error(
                        "Failed to close the library after its removal failed: \(closeError.localizedDescription)"
                    )
                }
                self.releaseLibrarySession(service)
                self.openLocalLibrary(id: libraryId)
                self.loadError = DisplayError(error)?
                    .addingContext(String(localized: "Couldn't remove library"))
            }
        }
    }
}
