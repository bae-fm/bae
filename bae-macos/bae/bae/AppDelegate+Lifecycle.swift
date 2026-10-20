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
        PopoverAnimation.disableForEveryPopover()
        guard let application = notification.object as? NSApplication else {
            preconditionFailure(
                "The launch notification did not contain NSApplication"
            )
        }
        prepareApplicationForLaunch(application)
        application.mainMenu = MainMenu.make(
            openLibrary: openLibraryMenu.menu,
            selectAll: selectAllTarget
        )
    }

    func prepareApplicationForLaunch(
        _ application: any ApplicationActivationPolicy
    ) {
        guard runtime != .preview else { return }
        Self.setRegularActivationPolicyIfNeeded(application)
    }

    static func setRegularActivationPolicyIfNeeded(
        _ application: any ApplicationActivationPolicy
    ) {
        if application.activationPolicy() == .regular {
            return
        }
        precondition(
            application.setActivationPolicy(.regular),
            "The application could not adopt the regular activation policy"
        )
    }

    func applicationDidFinishLaunching(_: Notification) {
        guard runtime.startsApplicationServices else { return }
        var keyringReady = true
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
                    "OAuth registration failed: \(error.localizedDescription)"
                )
                loadError = DisplayError(error)
            }
        #endif
        do {
            try initializeKeyring()
        }
        catch {
            // Discovery reads config.yaml and needs no keychain, so a
            // keyring failure must not cost the user the list of libraries
            // on this Mac — returning here handed a first-run create/join
            // wall to someone with libraries sitting right there. It does
            // cost them the open, since every library's keys live in the
            // keyring, so discovery runs and the welcome screen lists them
            // under this error.
            baeAppLogger.error(
                "Keyring initialization failed: \(error.localizedDescription)"
            )
            loadError = DisplayError(error)
            keyringReady = false
        }
        startWatchingForKeychainUnlock()
        observeDeviceWake()
        loadInitialState(canOpenLibraries: keyringReady)
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

    private func initializeKeyring() throws {
        try initKeyring(diagnostics: requiredApplicationServices.diagnostics)
    }

    /// AppKit's deferred-terminate flow, replacing the old fire-and-forget
    /// `applicationWillTerminate`: that method fired a detached `Task` and
    /// returned immediately, so the process could exit before the shutdown
    /// task's `persist_playback_state` write landed — losing the resume
    /// position on every quit that raced it. Returning `.terminateLater` here
    /// and replying only after `shutdown()` succeeds makes Quit wait for the
    /// save. A failure cancels termination and leaves the library open.
    func applicationShouldTerminate(_ sender: NSApplication)
        -> NSApplication.TerminateReply
    {
        guard let appService else {
            // No open library: closeLibrary() already shut its handle down
            // (or none was ever opened), so there's nothing left to save.
            return .terminateNow
        }
        prepareForLibraryShutdown()
        let shutdown = beginLibraryShutdown(appService)
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
        guard let appService else { return }
        reloadLibraries()
        // Coming back to the app is a moment the network may be back too.
        appService.reportNetworkChange()
    }

    /// Report the device waking to the open library: the network may be
    /// back after sleep, and sync's offline backoffs should end now rather
    /// than run out. Registered once, for the process's life; with no library
    /// open there is nothing to tell.
    private func observeDeviceWake() {
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.appService?.reportNetworkChange()
            }
        }
    }

    func application(_: NSApplication, open urls: [URL]) {
        guard runtime.startsApplicationServices else { return }
        guard let entry = appService?.importFolderEntry else { return }
        for url in urls {
            entry.take(url)
        }
    }
}
