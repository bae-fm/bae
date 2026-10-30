import AppKit
import BaeKit
import Synchronization
import os.log

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
    var loadError: LoadError?
    /// What launch found wrong. Set once, at launch, and kept for the run
    /// whatever opens or fails after.
    var launchProblems = LaunchProblems.none
    /// The welcome mode a menu item asked for; nil is the chooser.
    var welcomeInitialMode: WelcomeView.Mode?
    /// True once the first library has opened and replaced the bootstrap
    /// screens.
    var hasShell: Bool = false
    /// Presents the welcome sheet over an open library.
    var showAddLibrarySheet: Bool = false
    /// Every library on this device, for the File → Open Library submenu.
    var libraries: [BridgeLibrary] = [] {
        didSet { openLibraryMenu.show(libraries) }
    }
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
    /// Each window while it is open; closing one lets it go, and opening it
    /// again builds it anew.
    @ObservationIgnored
    var mainWindow: MainWindowController?
    @ObservationIgnored
    var settingsWindow: HostingWindowController?
    @ObservationIgnored
    var storageManagerWindow: HostingWindowController?
    /// The main menu's File ▸ Open Library.
    @ObservationIgnored
    let openLibraryMenu = OpenLibraryMenu()

    override convenience init() {
        self.init(runtime: AppRuntime(environment: baeAppProcessEnvironment))
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
    /// only when `canOpenLibraries` and no earlier removal was left
    /// unfinished: the welcome screen shows that removal, with Retry when
    /// files remain to delete.
    func loadInitialState(canOpenLibraries: Bool) {
        let host = requiredApplicationServices.host
        let removals = requiredApplicationServices.libraryRemovals
        Task {
            await removals.loadUnfinished()
            do {
                libraries = try await discoverInitialLibraries(
                    host: host,
                    environment: baeAppProcessEnvironment
                )
            }
            catch {
                // The welcome screen shows `loadError`.
                baeAppLogger.error(
                    "Failed to list libraries: \(String(reflecting: error))"
                )
                loadError = LoadError(.libraryList, error)
                screen = .welcome
                return
            }
            // Core lists the active library first; one whose settings can't
            // be read shows on the welcome, saying why.
            guard canOpenLibraries, removals.error == nil,
                let library = libraries.first, library.isOpenable
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
            if let recovery = AppScreen.recovering(
                libraryId: libraryId,
                from: error
            ) {
                screen = recovery
                // A screen that finishes a reset or restore says so itself.
                if case .resetRequired = recovery {
                    loadError = LoadError(.resetOffered, error)
                }
            }
            else {
                loadError = LoadError(.libraryOpen, error)
                if !hasShell {
                    // A failed bootstrap open returns to the welcome.
                    screen = .welcome
                }
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
                    try await self.close(open)
                }
                self.screen = .restoreRequired(libraryId: libraryId)
            }
            catch {
                baeAppLogger.error(
                    "Closing the library to restore it failed: \(String(reflecting: error))"
                )
                self.loadError = LoadError(.restoreClose, error)
                self.screen = .welcome
            }
        }
    }

    /// Show the one call that finishes a reset or restore of a library's
    /// copy on this Mac that stopped partway, from the welcome's list.
    func presentFinishing(libraryId: String, by: BridgeReplacedBy) {
        loadError = nil
        showAddLibrarySheet = false
        screen = .finishing(libraryId: libraryId, by: by)
    }

    /// Leave a reset or a finishing screen without running it.
    func leaveRecovery() {
        loadError = nil
        screen = hasShell ? .library : .welcome
    }

    /// Reset this device for a library, or finish a reset that stopped
    /// partway: close it if it is open, then load it from storage again under
    /// a fresh device id. The person saw the warning before choosing, so
    /// nothing is shown after. A failed reset returns to the screen it was
    /// started from, with why.
    func resetDevice(libraryId: String) {
        let host = requiredApplicationServices.host
        let deviceName = requiredApplicationServices.deviceName
        let startedFrom = screen
        let open = appService.flatMap { $0.libraryId == libraryId ? $0 : nil }
        if open != nil {
            prepareForLibraryShutdown()
        }
        loadError = nil
        screen = .loading
        resetSlot.replace {
            do {
                if let open {
                    try await self.close(open)
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
                    "Device reset failed: \(String(reflecting: error))"
                )
                self.loadError = LoadError(.deviceReset, error)
                self.screen = startedFrom
            }
        }
    }

    private func landOpenedService(_ service: AppService) {
        if let replaced = appService, replaced !== service {
            closeReplaced(replaced)
        }
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
            try await service.close()
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
            // The library is closed whether or not closing it failed, so the
            // session ends either way, showing why.
            baeAppLogger.error(
                "Failed to close library: \(failure.diagnostic)"
            )
            releaseLibrarySession(service)
            loadError = failure.displayedError.map { LoadError(.shutdown, $0) }
        }
    }

    func prepareForLibraryShutdown() {
        // An open still in flight must not replace this session.
        opener.cancel()
        renameSlot.cancel()
        forgetSlot.cancel()
    }

    /// Close the library another one replaced, off the main actor: releasing
    /// its handle here would close it on the main thread. Nothing waits on it,
    /// so a failure is logged.
    private func closeReplaced(_ replaced: AppService) {
        Task {
            do {
                try await replaced.close()
            }
            catch {
                baeAppLogger.error(
                    "Failed to close the library another replaced: \(String(reflecting: error))"
                )
            }
        }
    }

    /// Close `service`'s library and end its session. The library is closed
    /// whether or not closing it fails, so the session ends either way.
    private func close(_ service: AppService) async throws {
        defer { releaseLibrarySession(service) }
        try await service.close()
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
                    "Failed to list libraries: \(String(reflecting: $0))"
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
                    "Failed to rename \(libraryId): \(String(reflecting: $0))"
                )
                self.renameLibrarySheet?.error = DisplayError($0)
            }
        )
    }
}

extension AppDelegate {
    /// Remove the open library from this Mac, once the person confirmed what
    /// it loses: this Mac leaves the store, then its local copy is deleted,
    /// discarding what this Mac hasn't sent. The person's restore code stays
    /// in iCloud Keychain, so the welcome offers restoring the library again.
    /// A failed removal opens the library again.
    func forgetActiveLibrary() {
        guard let service = appService else {
            baeAppLogger.warning(
                "Ignoring remove-library request: no library is open."
            )
            return
        }
        prepareForLibraryShutdown()
        screen = .loading
        let removals = requiredApplicationServices.libraryRemovals
        forgetSlot.replace {
            do {
                let removal = try await service.removeLibrary()
                self.releaseLibrarySession(service)
                removals.record(removal)
                self.reloadLibraries()
            }
            catch {
                baeAppLogger.error(
                    "Failed to remove library: \(String(reflecting: error))"
                )
                await self.reopen(service, error: DisplayError(error))
            }
        }
    }

    /// Removal stopped the library's services before leaving the store; close
    /// what is left and open it again, showing why it stayed.
    private func reopen(_ service: AppService, error: DisplayError?) async {
        let libraryId = service.libraryId
        do {
            try await service.close()
        }
        catch let closeError {
            baeAppLogger.error(
                "Failed to close the library after its removal stopped: \(String(reflecting: closeError))"
            )
        }
        releaseLibrarySession(service)
        openLocalLibrary(id: libraryId)
        loadError = error.map { LoadError(.removal, $0) }
    }
}
