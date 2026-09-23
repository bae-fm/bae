import BaeKit
import SwiftUI
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
    // The host's OAuth client config and any error loading it. Present only in a
    // full build; a baeium (S3-only) build compiles out the OAuth flow entirely, so
    // there is nothing to load and no property to carry.
    #if BAE_OAUTH_PROVIDERS
    private let oauthLinking: OAuthLinking?
    private let oauthLinkingError: String?
    #endif
    private let startupError: String?
    /// The process-lifetime telemetry sink, built first at launch and held for
    /// the whole app run. `initKeyring` and the host require it.
    private let diagnostics: BridgeDiagnostics
    /// The process-lifetime host registrations, built right after the telemetry
    /// sink. Every library open, restore, join, and OAuth sign-in requires it.
    private let host: BridgeHost

    init() {
        #if BAE_OAUTH_PROVIDERS
        var loadedOAuthLinking: OAuthLinking?
        var oauthError: String?
        #endif
        // bae's directory lives under the Application Support container: iOS
        // app processes have no home directory of their own to put it under.
        let dataDirectory = Self.dataDirectory()
        let appDir = BridgeAppDir(home: dataDirectory.path)
        // Telemetry first, from compiled-in values only, so the sink exists for
        // every later launch step and any failure it reports.
        let diagnostics = BaeDiagnostics.configure(
            source: "ios",
            edition: appEdition,
            appDir: appDir
        )
        self.diagnostics = diagnostics
        let host = BaeHost.make(diagnostics: diagnostics, appDir: appDir)
        self.host = host
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
            // Hand Rust the CloudKit driver once. It can't build the driver itself
            // (it needs the platform CloudKit APIs); installing it is idempotent and
            // harmless for libraries that sync elsewhere, so it belongs here at the
            // composition root rather than at each library open.
            #if BAE_CLOUDKIT
            host.setCloudkitDriver(driver: CloudKitService.bae())
            #endif
        }
        catch {
            launchError = error.displayLine
        }

        #if BAE_OAUTH_PROVIDERS
        if launchError == nil {
            // Register the host's OAuth client creds (if a creds file is bundled) so
            // coven can build authorization URLs and refresh provider tokens during
            // sync. Absent file → cloud providers that need OAuth stay unavailable.
            do {
                loadedOAuthLinking = try OAuthLinking.load()
                try loadedOAuthLinking?.register(host: host)
            }
            catch {
                oauthError = error.displayLine
            }
        }
        oauthLinking = loadedOAuthLinking
        oauthLinkingError = oauthError
        #endif
        startupError = launchError
    }

    var body: some Scene {
        WindowGroup {
            #if BAE_OAUTH_PROVIDERS
            ContentView(
                oauthLinking: oauthLinking,
                oauthLinkingError: oauthLinkingError,
                startupError: startupError,
                diagnostics: diagnostics,
                host: host
            )
            .appAppearance()
            #else
            ContentView(
                startupError: startupError,
                diagnostics: diagnostics,
                host: host
            )
            .appAppearance()
            #endif
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
