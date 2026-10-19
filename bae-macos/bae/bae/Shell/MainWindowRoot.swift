import BaeKit
import SwiftUI

/// The primary window's content. It changes content and content-driven size
/// as the library opens and closes; the window itself is never replaced.
struct MainWindowRoot: View {
    let appDelegate: AppDelegate

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

    var body: some View {
        Group {
            if appDelegate.hasShell, let appService = appDelegate.appService {
                appService.installEnvironment(
                    libraryModals(
                        MainWindowChrome(loadError: appDelegate.loadError) {
                            detailContent
                        }
                        .navigationTitle(windowTitle)
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
        .environment(
            \.openSettingsWindow,
            OpenSettingsWindowAction { appDelegate.showSettingsWindow(nil) }
        )
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
