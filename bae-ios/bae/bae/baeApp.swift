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
    private let startupError: String?
    /// The process-lifetime telemetry sink, built first at launch and held for
    /// the whole app run. `initKeyring` and the host require it.
    private let diagnostics: BridgeDiagnostics
    /// The process-lifetime host registrations, built right after the telemetry
    /// sink. Every library open, restore, join, and provider sign-in requires it.
    private let host: BridgeHost
    /// The name this device registers under when it joins or sets up a
    /// library, which other devices show for it.
    private let deviceName: String

    init() {
        // bae's directory lives under the Application Support container: iOS
        // app processes have no home directory of their own to put it under.
        let dataDirectory = Self.dataDirectory()
        let appDir = BridgeAppDir(home: dataDirectory.path)
        // Telemetry first, from compiled-in values only, so the sink exists for
        // every later launch step and any failure it reports.
        let diagnostics = BaeDiagnostics.configure(
            source: "ios",
            edition: appEdition
        )
        self.diagnostics = diagnostics
        let host = BaeHost.make(diagnostics: diagnostics, appDir: appDir)
        self.host = host
        deviceName = UIDevice.current.name
        var launchError: String?
        do {
            BaeCrashReporting.configure(edition: appEdition)
            Logger.bae("BaeApp").info("application launched")
            try FileManager.default.createDirectory(
                at: dataDirectory,
                withIntermediateDirectories: true
            )
            // No `setCaCertDir` on iOS — the TLS stack uses Apple's trust roots.
            try initKeyring(diagnostics: diagnostics)
            #if BAE_OAUTH_PROVIDERS
            // The build's Dropbox and OneDrive applications and the system
            // sign-in sheet, before anything can restore, join or sign in.
            // Without a bundled config, provider sign-in stays unavailable.
            try OAuthLinking.load()?.register(host: host)
            #endif
        }
        catch {
            launchError = error.displayLine
        }
        startupError = launchError
    }

    var body: some Scene {
        WindowGroup {
            ContentView(
                startupError: startupError,
                diagnostics: diagnostics,
                host: host,
                deviceName: deviceName
            )
            .appAppearance()
        }
    }

    /// The app's Application Support directory. bae-core writes its library
    /// tree and config under here; launch creates it if absent.
    private static func dataDirectory() -> URL {
        FileManager.default
            .urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            )[0]
    }
}
