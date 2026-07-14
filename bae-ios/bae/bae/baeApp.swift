import BaeKit
import SwiftUI
import os.log

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
    /// the whole app run. `initKeyring` and every library open require it.
    private let diagnostics: BridgeDiagnostics

    init() {
        #if BAE_OAUTH_PROVIDERS
        var loadedOAuthLinking: OAuthLinking?
        var oauthError: String?
        #endif
        // Telemetry first, from compiled-in values only (no $HOME needed), so the
        // sink exists for every later launch step and any failure it reports.
        let diagnostics = BaeDiagnostics.configure(source: "ios")
        self.diagnostics = diagnostics
        var launchError: String?
        do {
            BaeCrashReporting.configure()
            Logger.bae("BaeApp").info("application launched")
            // App processes on iOS have no $HOME, which bae-core needs to locate its
            // data root (`~/.bae`). Point it at our Application Support container
            // before any library access (discover/restore/initApp) so those don't
            // fail with "could not determine home directory". `set_data_dir` sets
            // HOME, so it must run before `initKeyring`.
            setDataDir(path: try Self.dataDirectory())
            // No `setCaCertDir` on iOS — the TLS stack uses Apple's trust roots.
            initKeyring(diagnostics: diagnostics)
            // Hand Rust the CloudKit driver once. It can't build the driver itself
            // (it needs the platform CloudKit APIs); installing it is idempotent and
            // harmless for libraries that sync elsewhere, so it belongs here at the
            // composition root rather than at each library open.
            #if BAE_CLOUDKIT
            setCloudkitDriver(driver: CloudKitService.bae())
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
                try loadedOAuthLinking?.register()
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
                diagnostics: diagnostics
            )
            .tint(Theme.accent)
            #else
            ContentView(startupError: startupError, diagnostics: diagnostics)
                .tint(Theme.accent)
            #endif
        }
    }

    /// Absolute path to the app's Application Support directory, created if
    /// absent. bae-core writes its library tree and config under here.
    private static func dataDirectory() throws -> String {
        let fileManager = FileManager.default
        let base =
            fileManager.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            )[0]
        try fileManager.createDirectory(
            at: base,
            withIntermediateDirectories: true
        )
        return base.path
    }
}
