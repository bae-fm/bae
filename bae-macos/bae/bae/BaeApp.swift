import Combine
import Sparkle
import SwiftUI
import os.log

private let logger = Logger.bae("BaeApp")

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

    private static var isPreview: Bool {
        ProcessInfo.processInfo.environment["XCODE_RUNNING_FOR_PREVIEWS"] == "1"
    }

    init() {
        let startUpdater = !Self.isPreview
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
            return "\(name) — bae"
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

    /// WelcomeView constructed with the deep-link mode if the sidebar
    /// requested one, else the default chooser. Bound to the same
    /// callback for both paths.
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

    /// Detail-pane content while the shell is mounted. Switching
    /// libraries flips `screen` to `.loading` briefly; we render a
    /// ProgressView in the detail so the sidebar stays visible across
    /// the swap.
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
                // Cancelling a switch-to-locked-library returns to the library
                // that's still open behind the shell.
                onCancel: { appDelegate.screen = .library },
            )
        case .library:
            MainAppView()
        }
    }

    /// Sidebar + detail, kept mounted for the lifetime of the app once
    /// the first library opens. "+ Add..." in the sidebar presents
    /// `WelcomeView` as a sheet rather than swapping screens.
    @ViewBuilder
    private var shellContent: some View {
        NavigationSplitView(
            columnVisibility: Binding(
                get: { appDelegate.sidebarVisibility },
                set: { appDelegate.sidebarVisibility = $0 }
            )
        ) {
            LibrarySidebar(
                onOpen: { appDelegate.openLibrary($0) },
                onAddLibrary: { mode in
                    appDelegate.welcomeInitialMode = mode
                    appDelegate.showAddLibrarySheet = true
                },
                onRevealInFinder: { library in
                    SystemActions.revealInFinder(path: library.path)
                },
                onCopyLibraryId: { id in
                    SystemActions.copyToPasteboard(id)
                },
                librariesChanged: appDelegate.librariesChanged
                    .eraseToAnyPublisher(),
            )
            .navigationSplitViewColumnWidth(
                min: 180,
                ideal: 220,
                max: 320,
            )
        } detail: {
            detailContent
        }
        .sheet(
            isPresented: Binding(
                get: { appDelegate.showAddLibrarySheet },
                set: { appDelegate.showAddLibrarySheet = $0 }
            )
        ) {
            welcomeView
        }
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
        Window("bae", id: "main") {
            Group {
                if appDelegate.hasShell {
                    // The split view must be the window's structural root for
                    // its sidebar column to render on macOS; wrapping it in a
                    // stack collapses it to detail-only.
                    shellContent
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
            .background(Theme.background)
            .preferredColorScheme(.dark)
            .navigationTitle(windowTitle)
            .overlay(alignment: .bottom) {
                if let loadError = appDelegate.loadError {
                    Text(loadError)
                        .foregroundStyle(.red)
                        .padding()
                }
            }
            .overlay(alignment: .top) {
                if let libraries = appDelegate.quickSwitcher {
                    LibraryQuickSwitcher(
                        libraries: libraries,
                        onPick: { lib in
                            appDelegate.quickSwitcher = nil
                            appDelegate.openLibrary(lib)
                        },
                        onCancel: { appDelegate.quickSwitcher = nil },
                    )
                }
            }
            .environment(appDelegate.appService?.playbackStore)
            .environment(appDelegate.appService?.configStore)
            .environment(appDelegate.appService?.importStore)
            .environment(appDelegate.appService?.libraryStore)
            .environment(appDelegate.appService?.mediaPaths)
            .environment(appDelegate.appService?.playback)
            .environment(appDelegate.appService?.queue)
            .environment(appDelegate.appService?.previewAudio)
            .environment(appDelegate.appService?.library)
            .environment(appDelegate.appService?.releaseEditor)
            .environment(appDelegate.appService?.importer)
            .environment(appDelegate.appService?.sync)
            .environment(appDelegate.appService?.downloads)
            .environment(appDelegate.appService?.discogs)
            .environment(appDelegate.appService?.export)
            .environment(appDelegate.appService?.outboxStore)
            .environment(appDelegate.appService?.downloadStore)
            .environment(\.playbackPositionPublisher, playbackPublisher)
            .environment(\.previewProgressPublisher, previewPublisher)
            .environment(appDelegate.uiStore)
            .onAppear {
                #if !DEBUG
                    updaterController.updater.checkForUpdatesInBackground()
                #endif
            }
        }
        .windowStyle(.hiddenTitleBar)
        .commandsRemoved()
        Window("Storage Manager", id: "storage-manager") {
            if let appService = appDelegate.appService {
                StorageManagerView()
                    .environment(appService.libraryStore)
                    .environment(appService.mediaPaths)
                    .environment(appService.releaseEditor)
                    .environment(appService.library)
                    .environment(appService.outboxStore)
                    .environment(appService.downloadStore)
                    .environment(appService.sync)
                    .environment(appService.downloads)
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
        Settings {
            if let appService = appDelegate.appService {
                SettingsView(checkForUpdatesViewModel: checkForUpdatesViewModel)
                    .environment(appService.configStore)
                    .environment(appService.libraryStore)
                    .environment(appService.sync)
                    .environment(appService.discogs)
                    .environment(\.playbackPositionPublisher, playbackPublisher)
                    .environment(\.previewProgressPublisher, previewPublisher)
                    .environment(appDelegate.uiStore)
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
                    onCloseLibrary: { appDelegate.closeLibrary() },
                    onSwitchLibrary: { appDelegate.openQuickSwitcher() }
                )
            }
        }
    }
}

// MARK: - AppDelegate

@Observable
final class AppDelegate: NSObject, NSApplicationDelegate, @unchecked Sendable {
    var appService: AppService?
    var uiStore = UiStore()
    var screen: AppScreen = .loading
    var loadError: String?
    /// When the sidebar's "+ Add..." menu deep-links into a specific
    /// flow (Restore), this holds the requested initial mode for
    /// the welcome view about to be presented. `nil` lands on the
    /// default chooser.
    var welcomeInitialMode: WelcomeView.Mode?
    /// True when the sidebar+detail shell should be the root. Becomes
    /// true the first time any library is opened and stays true so
    /// switches don't unmount and remount the sidebar.
    var hasShell: Bool = false
    /// Drives the welcome sheet shown from the sidebar's "+ Add..."
    /// menu. Distinct from `.welcome` screen state, which only applies
    /// before any library has been opened.
    var showAddLibrarySheet: Bool = false
    /// Sidebar visibility, held here so SwiftUI's View > Show/Hide
    /// Sidebar menu item binds to a stable target.
    var sidebarVisibility: NavigationSplitViewVisibility = .automatic
    /// Fires whenever the set of libraries known to bae may have changed —
    /// after create / restore / switch — so the sidebar refetches
    /// in place. A one-shot signal, not observable state.
    @ObservationIgnored
    let librariesChanged = PassthroughSubject<Void, Never>()
    /// The in-flight library open, owned so a superseding open (a fast switch)
    /// can cancel it before it lands a stale library on `screen`/`appService`.
    @ObservationIgnored
    private var openTask: Task<Void, Never>?
    /// When set, the quick-switcher overlay is shown. Populated by the
    /// "Switch Library..." File-menu item.
    var quickSwitcher: [BridgeLibrary]?

    private static var isPreview: Bool {
        ProcessInfo.processInfo.environment["XCODE_RUNNING_FOR_PREVIEWS"] == "1"
    }

    func applicationDidFinishLaunching(_: Notification) {
        if !Self.isPreview {
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

    func application(_: NSApplication, open urls: [URL]) {
        for url in urls {
            var isDir: ObjCBool = false
            guard
                FileManager.default.fileExists(
                    atPath: url.path,
                    isDirectory: &isDir
                ),
                isDir.boolValue
            else {
                continue
            }
            do {
                try appService?.appHandle
                    .enqueueFolderScan(path: url.path, clearFirst: true)
            }
            catch {
                uiStore.showError(
                    "Scan failed: \(error.localizedDescription)"
                )
            }
            uiStore.navigateToImport()
        }
    }

    func applicationWillTerminate(_: Notification) {
        guard UserDefaults.standard.bool(forKey: "persistPlayback"),
            let appService
        else {
            return
        }
        appService.appHandle.shutdown()
    }

    // MARK: - Library lifecycle

    private func loadInitialState() {
        if Self.isPreview {
            screen = .welcome
            return
        }
        do {
            let libraries = try discoverLibraries()
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
                let handle = try await DetachedWork.run {
                    try initApp(
                        libraryId: libraryId,
                        positionUpdateIntervalMs: 200,
                    )
                }
                // A newer open may have superseded this one while `initApp`
                // ran; bail before touching screen/appService so the stale open
                // can't clobber the current one. `handle` drops here.
                try Task.checkCancellation()
                let cfg = handle.getConfig()
                if cfg.encryptionKeyStored, !handle.hasEncryptionKey() {
                    self.screen = .unlock(
                        libraryId: libraryId,
                        libraryName: cfg.libraryName,
                        fingerprint: cfg.encryptionKeyFingerprint,
                    )
                    return
                }
                let service = AppService(
                    appHandle: handle,
                    uiStore: store,
                    config: cfg
                )
                service.wireUp()
                if handle.isSyncReady() {
                    service.sync.storeRestoreCodeInKeychain(
                        libraryId: cfg.libraryId,
                        onError: { [store] in store.showError($0) }
                    )
                }
                self.appService = service
                self.screen = .library
                self.librariesChanged.send()
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

    /// Close the open library and return to the welcome chooser. Stops
    /// playback, drops the `AppService` — which releases the `AppHandle` and
    /// ends the core's UI-event subscription as the broadcast sender drops —
    /// and replaces `uiStore` so the next library opens with fresh navigation
    /// state. Flipping `hasShell` back to false routes the window from the
    /// sidebar shell to the bootstrap `WelcomeView`.
    func closeLibrary() {
        guard let service = appService else { return }
        // Cancel any open still in flight so a parked `initApp` can't resume past
        // its post-await cancellation check and write `.library`/`appService`
        // back after the close.
        openTask?.cancel()
        service.appHandle.shutdown()
        appService = nil
        uiStore = UiStore()
        welcomeInitialMode = nil
        quickSwitcher = nil
        loadError = nil
        screen = .welcome
        hasShell = false
    }

    /// Open the quick-switcher overlay populated with the current library
    /// snapshot. No-op when the library shell isn't mounted (loading, welcome,
    /// unlock) — there's nothing to switch to.
    func openQuickSwitcher() {
        guard case .library = screen else {
            return
        }
        do {
            quickSwitcher = try discoverLibraries()
        }
        catch {
            loadError = error.localizedDescription
        }
    }
}
