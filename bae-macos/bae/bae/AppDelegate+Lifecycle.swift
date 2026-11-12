import BaeKit
import SwiftUI

@MainActor
protocol ApplicationActivationPolicy: AnyObject {
    func activationPolicy() -> NSApplication.ActivationPolicy
    func setActivationPolicy(
        _ activationPolicy: NSApplication.ActivationPolicy
    ) -> Bool
}

extension NSApplication: ApplicationActivationPolicy {}

extension AppDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        guard runtime != .preview else { return }
        guard let application = notification.object as? NSApplication else {
            preconditionFailure(
                "The launch notification did not contain NSApplication"
            )
        }
        prepareApplicationForLaunch(application)
        application.mainMenu = MainMenu.make(openLibrary: openLibraryMenu.menu)
    }

    func prepareApplicationForLaunch(
        _ application: any ApplicationActivationPolicy
    ) {
        switch runtime {
        case .preview:
            return
        case .testHost:
            Self.adoptActivationPolicyIfNeeded(.accessory, application)
        case .application:
            Self.adoptActivationPolicyIfNeeded(.regular, application)
        }
    }

    /// A test host is an accessory app, with no Dock icon or menu bar, so the
    /// copy of bae a test run starts is not mistaken for the person's own; it
    /// can still make a window key and activate, as the window shots need.
    static func adoptActivationPolicyIfNeeded(
        _ policy: NSApplication.ActivationPolicy,
        _ application: any ApplicationActivationPolicy
    ) {
        if application.activationPolicy() == policy {
            return
        }
        precondition(
            application.setActivationPolicy(policy),
            "The application could not adopt the \(policy) activation policy"
        )
    }

    func applicationDidFinishLaunching(_: Notification) {
        guard runtime.startsApplicationServices else { return }
        var signInSettings: DisplayError?
        // Telemetry is already up (built at delegate construction, from
        // compiled-in values only), so every step from here on has a sink
        // for any failure it reports.
        BaeCrashReporting.configure(edition: baeAppEdition)
        baeAppLogger.info("application launched")
        #if BAE_OAUTH_PROVIDERS
            // The build's provider applications, before anything can restore,
            // join or sign in; coven's desktop sign-in presents them. Without
            // a bundled config, provider sign-in stays unavailable.
            do {
                try OAuthLinking.load()?
                    .register(host: requiredApplicationServices.host)
            }
            catch {
                baeAppLogger.error(
                    "OAuth registration failed: \(String(reflecting: error))"
                )
                signInSettings = DisplayError(error)
            }
        #endif
        launchProblems = LaunchProblems(signInSettings: signInSettings)
        startWatchingForKeychainUnlock()
        #if BAE_DEV_RESET
            if let profilePath = AppRuntime.devResetProfilePath(
                arguments: CommandLine.arguments
            ) {
                runDevReset(profilePath: profilePath)
            }
            else {
                loadInitialState()
            }
        #else
            loadInitialState()
        #endif
        showMainWindow()
        #if !DEBUG
            requiredApplicationServices.updaterController.updater
                .checkForUpdatesInBackground()
        #endif
    }

    /// A Dock click with no window open presents the primary window, as
    /// launch does.
    func applicationShouldHandleReopen(
        _: NSApplication,
        hasVisibleWindows: Bool
    ) -> Bool {
        guard runtime.startsApplicationServices, !hasVisibleWindows else {
            return true
        }
        showMainWindow()
        return false
    }

    /// AppKit's deferred-terminate flow, replacing the old fire-and-forget
    /// `applicationWillTerminate`: that method fired a detached `Task` and
    /// returned immediately, so the process could exit before the shutdown
    /// task's `persist_playback_state` write landed — losing the resume
    /// position on every quit that raced it. Returning `.terminateLater` here
    /// and replying only after closing the library succeeds makes Quit wait
    /// for the save and for the store to close. A failure cancels termination
    /// and shows why on the welcome screen.
    func applicationShouldTerminate(_ sender: NSApplication)
        -> NSApplication.TerminateReply
    {
        guard let appService else {
            // No open library: closing it already ended its handle (or none
            // was ever opened), so there's nothing left to save.
            return .terminateNow
        }
        prepareForLibraryShutdown()
        // Quitting shows no screen of the close's steps.
        let shutdown = beginLibraryShutdown(appService, showing: nil)
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
        guard runtime.startsApplicationServices else { return }
        // Before the `appService` gate: having no service is the whole state.
        retryOpenIfKeychainWasLocked(trigger: "app activation")
        // Refresh the library list so the Open Library submenu reflects
        // libraries created, renamed, or removed elsewhere. Only meaningful
        // once a library is open — the menu that consumes it exists only then.
        guard appService != nil else { return }
        reloadLibraries()
    }

    func application(_: NSApplication, open urls: [URL]) {
        guard runtime.startsApplicationServices else { return }
        guard let entry = appService?.importFolderEntry else { return }
        for url in urls {
            entry.take(url)
        }
    }
}
