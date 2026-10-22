import BaeKit
import Combine
import SwiftUI

/// The primary window's content. It changes content and content-driven size
/// as the library opens and closes; the window itself is never replaced.
struct MainWindowRoot: View {
    let appDelegate: AppDelegate
    /// View ▸ Search, for the title bar's search field.
    let searchFocusRequests: PassthroughSubject<Void, Never>
    /// Edit ▸ Select All, for the import list.
    let shownRowsSelectAll: ShownRowsSelectAll

    private var applicationServices: ApplicationServices {
        appDelegate.requiredApplicationServices
    }

    var body: some View {
        Group {
            if appDelegate.hasShell, let appService = appDelegate.appService {
                appService.installEnvironment(
                    libraryModals(
                        MainWindowChrome(
                            launchProblems: appDelegate.launchProblems,
                            loadError: appDelegate.loadError
                        ) {
                            detailContent
                        }
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
        .mainWindowEnvironment(
            openSettingsWindow: OpenSettingsWindowAction {
                appDelegate.showSettingsWindow(nil)
            },
            searchFocusRequests: searchFocusRequests,
            shownRowsSelectAll: shownRowsSelectAll
        )
    }

    /// The welcome flow in the mode a menu item asked for, showing
    /// `loadError` and `launchProblems` (none in the Add Library sheet, whose
    /// failures the shell shows).
    private func welcomeView(
        loadError: LoadError?,
        launchProblems: LaunchProblems
    ) -> some View {
        Group {
            if let mode = appDelegate.welcomeInitialMode {
                WelcomeView(
                    onLibraryReady: appDelegate.openOpenedLibrary,
                    onOpenLibrary: appDelegate.openLocalLibrary(id:),
                    onFinishReplacement: appDelegate.presentFinishing,
                    initialMode: mode,
                    canDeleteActiveLibrary: !appDelegate.hasShell
                )
            }
            else {
                WelcomeView(
                    onLibraryReady: appDelegate.openOpenedLibrary,
                    onOpenLibrary: appDelegate.openLocalLibrary(id:),
                    onFinishReplacement: appDelegate.presentFinishing,
                    loadError: loadError,
                    launchProblems: launchProblems,
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
            deviceResetView(libraryId: libraryId, prompt: .offered)
        case .restoreRequired(let libraryId):
            deviceRestoreView(libraryId: libraryId)
        case .resetUnfinished(let libraryId):
            deviceResetView(libraryId: libraryId, prompt: .unfinished)
        case .restoreUnfinished(let libraryId):
            finishRestoreView(libraryId: libraryId)
        case .keychainLocked:
            KeychainLockedView(onRetry: appDelegate.retryKeychainOpen)
        case .library:
            MainAppView()
        }
    }

    private func deviceResetView(
        libraryId: String,
        prompt: DeviceResetView.Prompt
    ) -> some View {
        DeviceResetView(
            prompt: prompt,
            error: appDelegate.loadError,
            onReset: { appDelegate.resetDevice(libraryId: libraryId) },
            onCancel: appDelegate.leaveRecovery
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

    /// The person's current restore code, to finish a restore of this
    /// device's copy that stopped partway; the library can't open until it
    /// finishes, so going back leaves it as it is.
    private func finishRestoreView(libraryId: String) -> some View {
        BootstrapCodeView(
            form: applicationServices.librarySetup.bootstrapForm(
                entry: .finishRestore(libraryId: libraryId),
                onLibraryReady: appDelegate.openOpenedLibrary
            ),
            onBack: appDelegate.leaveRecovery
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
                welcomeView(loadError: nil, launchProblems: .none)
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
            welcomeView(
                loadError: appDelegate.loadError,
                launchProblems: appDelegate.launchProblems
            )
        case .resetRequired(let libraryId):
            deviceResetView(libraryId: libraryId, prompt: .offered)
        case .restoreRequired(let libraryId):
            deviceRestoreView(libraryId: libraryId)
        case .resetUnfinished(let libraryId):
            deviceResetView(libraryId: libraryId, prompt: .unfinished)
        case .restoreUnfinished(let libraryId):
            finishRestoreView(libraryId: libraryId)
        case .keychainLocked:
            KeychainLockedView(onRetry: appDelegate.retryKeychainOpen)
        case .library:
            // Bootstrap content stays independent of the service environment.
            Spacer()
            ProgressView()
            Spacer()
        }
    }

    /// Loading takes the library window's size, fitted to the screen;
    /// everything else the welcome size.
    private var bootstrapWindowSize: CGSize {
        if case .loading = appDelegate.screen {
            return MainWindow.sizeOnMainScreen
        }
        return WelcomeWindow.size
    }
}

extension View {
    /// What the primary window gives its content beyond the library's
    /// services: the shell's Settings window, View ▸ Search, and Edit ▸
    /// Select All.
    fileprivate func mainWindowEnvironment(
        openSettingsWindow: OpenSettingsWindowAction,
        searchFocusRequests: PassthroughSubject<Void, Never>,
        shownRowsSelectAll: ShownRowsSelectAll
    ) -> some View {
        self
            .environment(\.openSettingsWindow, openSettingsWindow)
            .environment(\.searchFocusRequests, searchFocusRequests)
            .environment(\.shownRowsSelectAll, shownRowsSelectAll)
    }
}

#if DEBUG
    import os.log

    private let mainWindowPreviewLogger = Logger.bae("MainWindowPreview")

    extension View {
        /// The primary window's values for a preview of its content, which
        /// has no Settings window to open and no menu to search or select
        /// all from.
        func mainWindowPreviewEnvironment() -> some View {
            self.mainWindowEnvironment(
                openSettingsWindow: OpenSettingsWindowAction {
                    mainWindowPreviewLogger.info(
                        "Settings asked for in a preview"
                    )
                },
                searchFocusRequests: PassthroughSubject(),
                shownRowsSelectAll: ShownRowsSelectAll()
            )
        }
    }
#endif
