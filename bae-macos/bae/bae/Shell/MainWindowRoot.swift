import BaeKit
import Combine
import SwiftUI

/// The primary window's content. It changes content and content-driven size
/// as the library opens and closes; the window itself is never replaced.
struct MainWindowRoot: View {
    let appDelegate: AppDelegate
    /// View ▸ Search, for the title bar's search field.
    let searchFocusRequests: PassthroughSubject<Void, Never>

    private var applicationServices: ApplicationServices {
        appDelegate.requiredApplicationServices
    }

    var body: some View {
        Group {
            if let appService = appDelegate.appService {
                appService.installEnvironment(
                    libraryModals(
                        MainWindowChrome {
                            detailContent
                        }
                        .environment(
                            \.launchProblems,
                            appDelegate.launchProblems
                        )
                        .environment(
                            \.libraryLoadError,
                            appDelegate.loadError
                        )
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
            openStorageManager: OpenStorageManagerAction {
                appDelegate.showStorageManager(nil)
            },
            searchFocusRequests: searchFocusRequests
        )
    }

    /// The welcome flow in the mode a menu item asked for, showing
    /// `loadError` and `launchProblems` (none in the Add Library sheet, whose
    /// failures the shell shows).
    private func welcomeView(
        loadError: LoadError?,
        launchProblems: LaunchProblems
    ) -> some View {
        WelcomeView(
            onLibraryReady: appDelegate.openOpenedLibrary,
            onOpenLibrary: appDelegate.openLocalLibrary(id:),
            onFinishReplacement: appDelegate.presentFinishing,
            loadError: loadError,
            launchProblems: launchProblems,
            initialMode: appDelegate.welcomeInitialMode ?? .choose,
            canDeleteActiveLibrary: !appDelegate.hasShell
        )
        .environment(applicationServices.librarySetup)
        .environment(applicationServices.libraryRemovals)
    }

    /// Main-window content while a library is open.
    @ViewBuilder
    private var detailContent: some View {
        switch appDelegate.screen {
        case .loading(let work):
            StepProgressLine(steps: appDelegate.closingSteps?.steps) {
                StepProgressLine(steps: appDelegate.openProgress?.steps) {
                    if let reset = appDelegate.resetSteps,
                        let status = reset.status
                    {
                        StepProgressLine(steps: reset.steps) {
                            BootstrapStatusView(status: status)
                        }
                    }
                    else {
                        WaitingIndicator(Text(work.title))
                    }
                }
            }
            .frame(maxWidth: StepProgressLayout.width)
        case .home:
            MainAppView()
        case .resetRequired(let libraryId):
            deviceResetView(
                prompt: .offered,
                onReset: { appDelegate.resetDevice(libraryId: libraryId) }
            )
        case .restoreRequired(let libraryId):
            deviceRestoreView(libraryId: libraryId)
        case .resetUnfinished(let libraryId):
            deviceResetView(
                prompt: .unfinished,
                onReset: { appDelegate.finishReset(libraryId: libraryId) }
            )
        case .restoreUnfinished(let libraryId):
            finishRestoreView(libraryId: libraryId)
        case .keychainLocked:
            KeychainLockedView(onRetry: appDelegate.retryKeychainOpen)
        }
    }

    private func deviceResetView(
        prompt: DeviceResetView.Prompt,
        onReset: @escaping () -> Void
    ) -> some View {
        DeviceResetView(
            prompt: prompt,
            error: appDelegate.loadError,
            onReset: onReset,
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

    /// Finishing a restore of this device's copy that stopped partway: it
    /// begins at once from what core kept, and the person's current restore
    /// code finishes it too. The library can't open until it finishes, so
    /// going back leaves it as it is.
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

    /// Main-window content while no library is open.
    @ViewBuilder
    private var bootstrapContent: some View {
        switch appDelegate.screen {
        case .loading(let work):
            Spacer()
            StepProgressLine(steps: appDelegate.setupSteps?.steps) {
                StepProgressLine(steps: appDelegate.openProgress?.steps) {
                    if let reset = appDelegate.resetSteps,
                        let status = reset.status
                    {
                        StepProgressLine(steps: reset.steps) {
                            BootstrapStatusView(status: status)
                        }
                    }
                    else {
                        WaitingIndicator(Text(work.title))
                    }
                }
            }
            .frame(maxWidth: StepProgressLayout.width)
            Spacer()
        case .home:
            welcomeView(
                loadError: appDelegate.loadError,
                launchProblems: appDelegate.launchProblems
            )
        case .resetRequired(let libraryId):
            deviceResetView(
                prompt: .offered,
                onReset: { appDelegate.resetDevice(libraryId: libraryId) }
            )
        case .restoreRequired(let libraryId):
            deviceRestoreView(libraryId: libraryId)
        case .resetUnfinished(let libraryId):
            deviceResetView(
                prompt: .unfinished,
                onReset: { appDelegate.finishReset(libraryId: libraryId) }
            )
        case .restoreUnfinished(let libraryId):
            finishRestoreView(libraryId: libraryId)
        case .keychainLocked:
            KeychainLockedView(onRetry: appDelegate.retryKeychainOpen)
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
    /// services: the shell's Settings window and View ▸ Search.
    fileprivate func mainWindowEnvironment(
        openSettingsWindow: OpenSettingsWindowAction,
        openStorageManager: OpenStorageManagerAction,
        searchFocusRequests: PassthroughSubject<Void, Never>
    ) -> some View {
        self
            .environment(\.openSettingsWindow, openSettingsWindow)
            .environment(\.openStorageManager, openStorageManager)
            .environment(\.searchFocusRequests, searchFocusRequests)
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
                openStorageManager: OpenStorageManagerAction {
                    mainWindowPreviewLogger.info(
                        "The Storage Manager asked for in a preview"
                    )
                },
                searchFocusRequests: PassthroughSubject()
            )
        }
    }
#endif
