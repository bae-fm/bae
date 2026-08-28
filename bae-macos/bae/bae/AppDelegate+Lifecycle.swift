import BaeKit
import SwiftUI

extension AppDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        guard runtime.startsApplicationServices else { return }
        guard let application = notification.object as? NSApplication else {
            preconditionFailure(
                "The launch notification did not contain NSApplication"
            )
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
        // Hand Rust the CloudKit driver once. It can't build the driver
        // itself (it needs the platform CloudKit APIs); installing it is
        // idempotent and harmless for libraries that sync elsewhere, so it
        // belongs here at the composition root rather than at each open.
        #if BAE_CLOUDKIT
            setCloudkitDriver(driver: CloudKitService.bae())
        #endif
        startWatchingForKeychainUnlock()
        loadInitialState(canOpenLibraries: keyringReady)
    }

    private func initializeKeyring() throws {
        #if DEBUG
            if AppRuntime.usesTestKeyring(
                environment: baeAppProcessEnvironment
            ) {
                initTestKeyring()
                return
            }
        #endif
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
        guard appService != nil else { return }
        reloadLibraries()
    }

    func application(_: NSApplication, open urls: [URL]) {
        guard runtime.startsApplicationServices else { return }
        for url in urls {
            addWatchedFolderFromOpenURL(url)
        }
    }

    private func addWatchedFolderFromOpenURL(_ url: URL) {
        var isDir: ObjCBool = false
        guard
            FileManager.default.fileExists(
                atPath: url.path,
                isDirectory: &isDir
            ),
            isDir.boolValue
        else {
            return
        }
        Task {
            do {
                try await appService?.addWatchedFolder(path: url.path)
            }
            catch {
                guard let displayed = DisplayError(error) else { return }
                uiStore.showError(
                    displayed.addingContext(
                        String(localized: "Couldn't add folder")
                    )
                )
            }
        }
        uiStore.navigateToImport()
    }
}
