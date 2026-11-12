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
            let created = try await host.createLibrary(
                name: nil,
                creatorName: "UI Test",
                setup: BridgeLibrarySetup(host: host)
            )
            let app = try created.open(
                positionUpdateIntervalMs: 200,
                restorePlayback: false
            )
            try app.closeLibrary(closing: nil)
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
    var screen: AppScreen = .loading(.findingLibraries)
    /// The library open in flight by id, whose steps the loading screen
    /// shows; nil while nothing opens one.
    var openProgress: LibraryOpenProgress?
    /// The setup call in flight before any library opens, such as a dev
    /// reset, whose steps the loading screen shows; nil while none runs.
    var setupSteps: LibrarySetupSteps?
    /// A device reset in flight, whose loading of the library the loading
    /// screen shows; nil while no reset runs.
    var resetSteps: DeviceResetSteps?
    /// A close of the open library in flight, whose steps the loading screen
    /// shows; nil while none runs.
    var closingSteps: LibraryClosingSteps?
    var loadError: LoadError?
    /// What launch found wrong. Set once, at launch, and kept for the run
    /// whatever opens or fails after.
    var launchProblems = LaunchProblems.none
    /// The welcome mode a menu item asked for; nil is the chooser.
    var welcomeInitialMode: WelcomeView.Mode?
    /// Whether a library is open, so the window shows its shell in place of
    /// the bootstrap screens.
    var hasShell: Bool { appService != nil }
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
            makeHandle: { libraryId, opening in
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
                            host: host,
                            opening: opening
                        )
                    }
                #endif
                return try initApp(
                    libraryId: libraryId,
                    positionUpdateIntervalMs: 200,
                    restorePlayback: restorePlayback,
                    // Telemetry is up before the library opens.
                    host: host,
                    opening: opening
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
            screen = .home
        }
    }

    /// Open Rename Library on the library's name as it stands now.
    func presentRenameLibrary() {
        guard let appService else {
            preconditionFailure("Rename Library is disabled without a library")
        }
        let libraryId = appService.libraryId
        let names = appService.libraryNameStore
        renameSlot.replace(
            "name read of \(libraryId)",
            work: { try await names.current() },
            onSuccess: { name in
                self.renameLibrarySheet = RenameLibrarySheetState(
                    id: libraryId,
                    newName: name
                )
            },
            onError: {
                baeAppLogger.error(
                    "Failed to read the name of \(libraryId): \(String(reflecting: $0))"
                )
                if let shown = DisplayError($0) { appService.showError(shown) }
            }
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
    /// only when no earlier removal was left unfinished: the welcome screen
    /// shows that removal, with Retry when files remain to delete.
    func loadInitialState() {
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
                screen = .home
                return
            }
            // Core lists the active library first; one whose settings can't
            // be read shows on the welcome, saying why.
            guard removals.unfinishedError == nil,
                let library = libraries.first, library.isOpenable
            else {
                screen = .home
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
        screen = .loading(.opening)
        let progress = LibraryOpenProgress(
            host: requiredApplicationServices.host
        )
        openProgress = progress
        opener.open(libraryId: libraryId, progress: progress) {
            [weak self] outcome in
            guard let self else { return }
            // A newer open owns the loading screen, and its own steps.
            if openProgress === progress {
                openProgress = nil
            }
            applyOpenOutcome(outcome, libraryId: libraryId, warning: nil)
        }
    }

    /// Open the store a create, restore, join or device reset returned,
    /// showing the error a partly finished restore or join reported once the
    /// library is up.
    func openOpenedLibrary(_ library: OpenedLibrary, warning: BridgeError?) {
        loadError = nil
        screen = .loading(.opening)
        // The store is already open, so no open runs a step.
        openProgress = nil
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
            let next = AppScreen.afterFailedOpen(
                libraryId: libraryId,
                from: error
            )
            switch next {
            case .resetRequired:
                screen = next
                loadError = LoadError(.resetOffered, error)
            case .home:
                screen = next
                loadError = LoadError(
                    .libraryOpen,
                    error,
                    libraryId: libraryId
                )
            case .resetUnfinished(let unfinished):
                // Finishing needs nothing from the person, and only it opens
                // the library.
                finishReset(libraryId: unfinished)
            default:
                // A screen that finishes a restore begins it, and says so
                // itself.
                screen = next
            }
        }
    }

    /// Show the screen that offers resetting this device for a library, from
    /// Settings.
    func presentDeviceReset(libraryId: String) {
        loadError = nil
        screen = .resetRequired(libraryId: libraryId)
    }

    /// Close the library and ask for the person's current restore code, to
    /// restore it over this device's copy after another of their devices
    /// replaced their keys. Going back opens the library again as it was.
    func presentDeviceRestore(libraryId: String) {
        loadError = nil
        guard
            let open = appService.flatMap({
                $0.libraryId == libraryId ? $0 : nil
            })
        else {
            screen = .restoreRequired(libraryId: libraryId)
            return
        }
        prepareForLibraryShutdown()
        screen = .loading(.closing)
        resetSlot.replace {
            do {
                try await self.close(open)
                self.screen = .restoreRequired(libraryId: libraryId)
            }
            catch {
                baeAppLogger.error(
                    "Closing the library to restore it failed: \(String(reflecting: error))"
                )
                self.loadError = LoadError(.restoreClose, error)
                self.screen = .home
            }
        }
    }

    /// Finish a reset or restore of a library's copy on this Mac that
    /// stopped partway, from the welcome's list: a reset at once, and a
    /// restore on the screen that finishes it, which begins at once.
    func presentFinishing(libraryId: String, by: BridgeReplacedBy) {
        loadError = nil
        showAddLibrarySheet = false
        switch by {
        case .reset:
            finishReset(libraryId: libraryId)
        case .restore:
            screen = .restoreUnfinished(libraryId: libraryId)
        }
    }

    /// Leave a reset or a finishing screen without running it.
    func leaveRecovery() {
        loadError = nil
        screen = .home
    }

    /// Reset from cloud for a library: close it if it is open, then load it
    /// from storage again under a fresh device id. The person saw the warning
    /// before choosing, so nothing is shown after. A failed reset returns to
    /// the screen it was started from, with why.
    func resetDevice(libraryId: String) {
        let host = requiredApplicationServices.host
        let deviceName = requiredApplicationServices.deviceName
        let startedFrom = screen
        let open = appService.flatMap { $0.libraryId == libraryId ? $0 : nil }
        if open != nil {
            prepareForLibraryShutdown()
        }
        loadError = nil
        screen = .loading(open == nil ? .resetting : .closing)
        resetSlot.replace {
            do {
                if let open {
                    try await self.close(open)
                    self.screen = .loading(.resetting)
                }
                let steps = DeviceResetSteps(host: host)
                self.resetSteps = steps
                defer {
                    steps.stop()
                    self.resetSteps = nil
                }
                let reset = try await host.resetDevice(
                    libraryId: libraryId,
                    deviceName: deviceName,
                    setup: steps.setup
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

    /// Finish a reset of this Mac's copy of a library that stopped partway,
    /// from what core kept, with no warning: the person accepted it when the
    /// reset began. The library can't be open. Once the reset finishes the
    /// library opens; a failed finish lands on the screen that finishes it,
    /// saying why.
    func finishReset(libraryId: String) {
        let host = requiredApplicationServices.host
        loadError = nil
        showAddLibrarySheet = false
        screen = .loading(.resetting)
        resetSlot.replace {
            let steps = DeviceResetSteps(host: host)
            self.resetSteps = steps
            defer {
                steps.stop()
                self.resetSteps = nil
            }
            do {
                let reset = try await host.finishReset(
                    libraryId: libraryId,
                    setup: steps.setup
                )
                self.openOpenedLibrary(reset.library, warning: reset.warning)
            }
            catch {
                baeAppLogger.error(
                    "Finishing the device reset failed: \(String(reflecting: error))"
                )
                self.loadError = LoadError(.deviceReset, error)
                self.screen = .resetUnfinished(libraryId: libraryId)
            }
        }
    }

    private func landOpenedService(_ service: AppService) {
        if let replaced = appService, replaced !== service {
            closeReplaced(replaced)
        }
        appService = service
        screen = .home
        service.reportScreen(.library)
        reloadLibraries()
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
            deviceName: requiredApplicationServices.deviceName,
            host: requiredApplicationServices.host
        )
        service.wireUp()
        return service
    }

    /// Close the open library and return to the welcome once it has shut
    /// down.
    func closeLibrary() {
        guard let service = appService else { return }
        prepareForLibraryShutdown()
        screen = .loading(.closing)
        let steps = LibraryClosingSteps(host: requiredApplicationServices.host)
        closingSteps = steps
        beginLibraryShutdown(service, showing: steps)
    }

    /// Close `service`'s library once, whoever asks first, writing its steps
    /// to `closing` when a screen shows them.
    @discardableResult
    func beginLibraryShutdown(
        _ service: AppService,
        showing closing: LibraryClosingSteps?
    ) -> Task<LibraryShutdownResult, Never> {
        let attempt = shutdownCoordinator.begin(for: service) {
            try await service.close(showing: closing)
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
        closingSteps = nil
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
    }

    /// Close the library another one replaced, off the main actor: releasing
    /// its handle here would close it on the main thread. Nothing waits on it,
    /// so a failure is shown on the alert, naming the library.
    private func closeReplaced(_ replaced: AppService) {
        Task {
            // The name is read first: it can't be once the library is closed.
            let name: String
            do {
                name = try await replaced.libraryNameStore.current()
            }
            catch {
                baeAppLogger.error(
                    "Failed to read the name of the library another replaced: \(String(reflecting: error))"
                )
                name = replaced.libraryId
            }
            do {
                try await replaced.close(showing: nil)
            }
            catch {
                baeAppLogger.error(
                    "Failed to close the library another replaced: \(String(reflecting: error))"
                )
                guard let failure = DisplayError(error) else { return }
                uiStore.showError(
                    DisplayError(
                        line: localizedCoreString(
                            "ui.load_error.shutdown.title"
                        ),
                        detail: "\(name)\n\(failure.detail ?? failure.line)"
                    )
                )
            }
        }
    }

    /// Close `service`'s library, showing its steps on the loading screen,
    /// and end its session. The library is closed whether or not closing it
    /// fails, so the session ends either way.
    private func close(_ service: AppService) async throws {
        let steps = LibraryClosingSteps(host: requiredApplicationServices.host)
        closingSteps = steps
        defer {
            closingSteps = nil
            releaseLibrarySession(service)
        }
        try await service.close(showing: steps)
    }

    private func releaseLibrarySession(_ service: AppService) {
        guard appService === service else { return }
        service.deactivateMediaControls()
        appService = nil
        uiStore = UiStore()
        welcomeInitialMode = nil
        renameLibrarySheet = nil
        loadError = nil
        screen = .home
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
