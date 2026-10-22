import BaeKit
import SwiftUI
import UIKit
import os.log

private let appEdition: AppEdition = {
    #if BAE_OAUTH_PROVIDERS
    .bae
    #else
    .baeium
    #endif
}()

@main
struct BaeApp: App {
    /// The process-lifetime telemetry sink, built first at launch and held for
    /// the whole app run. `initKeyring` and the host require it.
    private let diagnostics: BridgeDiagnostics
    /// The process-lifetime host registrations every library open, restore,
    /// join, and provider sign-in requires, or why launch stopped first.
    private let launch: AppLaunch
    /// The name this device registers under when it joins or sets up a
    /// library, which other devices show for it.
    private let deviceName: String

    init() {
        // Telemetry first, from compiled-in values only, so the sink exists for
        // every later launch step and any failure it reports.
        let diagnostics = BaeDiagnostics.configure(
            source: "ios",
            edition: appEdition
        )
        self.diagnostics = diagnostics
        deviceName = UIDevice.current.name
        launch = Self.launch(diagnostics: diagnostics)
    }

    var body: some Scene {
        WindowGroup {
            ContentView(
                launch: launch,
                diagnostics: diagnostics,
                deviceName: deviceName
            )
            .appAppearance()
        }
    }

    /// Bring up crash reporting, bae's directory, the host, and the keychain,
    /// stopping at the first that fails, named by its step.
    private static func launch(diagnostics: BridgeDiagnostics) -> AppLaunch {
        BaeCrashReporting.configure(edition: appEdition)
        Logger.bae("BaeApp").info("application launched")
        let host: BridgeHost
        do {
            // bae's directory lives under the Application Support container:
            // iOS app processes have no home directory of their own to put it
            // under. Building it creates it.
            let appDir = try BridgeAppDir(home: dataDirectory().path)
            host = BaeHost.make(diagnostics: diagnostics, appDir: appDir)
        }
        catch {
            return failed(.startup, error)
        }
        do {
            // No `setCaCertDir` on iOS — the TLS stack uses Apple's trust roots.
            try initKeyring(diagnostics: diagnostics)
        }
        catch {
            return failed(.secureStorage, error)
        }
        #if BAE_OAUTH_PROVIDERS
        do {
            // The build's Dropbox and OneDrive applications and the system
            // sign-in sheet, before anything can restore, join or sign in.
            // Without a bundled config, provider sign-in stays unavailable.
            try OAuthLinking.load()?.register(host: host)
        }
        catch {
            return failed(.signInSettings, error)
        }
        #endif
        return .ready(host)
    }

    /// The launch stopped at `step`. Only a cancellation has no line, and
    /// nothing at launch can be cancelled.
    private static func failed(
        _ step: ScreenFailure.Step,
        _ error: any Error
    ) -> AppLaunch {
        guard let failure = ScreenFailure(step, error) else {
            preconditionFailure("A launch step reported a cancellation: \(error)")
        }
        return .failed(failure)
    }

    /// The app's Application Support directory, which bae's directory is
    /// under.
    private static func dataDirectory() -> URL {
        FileManager.default
            .urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            )[0]
    }
}
