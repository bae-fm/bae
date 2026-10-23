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

private let logger = Logger.bae("BaeApp")

@main
struct BaeApp: App {
    /// The process-lifetime host, or the launch step that stopped first.
    private let launch: AppLaunch

    init() {
        // Telemetry first, from compiled-in values only, so the sink exists for
        // every later launch step. The host and the keychain hold it from here.
        let diagnostics = BaeDiagnostics.configure(
            source: "ios",
            edition: appEdition
        )
        launch = Self.launch(diagnostics: diagnostics)
    }

    var body: some Scene {
        WindowGroup {
            LaunchView(launch: launch)
        }
    }

    /// Bring up crash reporting, bae's directory, the host, and the keychain,
    /// stopping at the first that fails, named by its step.
    private static func launch(diagnostics: BridgeDiagnostics) -> AppLaunch {
        BaeCrashReporting.configure(edition: appEdition)
        logger.info("application launched")
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
        return .ready(host)
    }

    private static func failed(
        _ step: AppLaunch.Step,
        _ error: any Error
    ) -> AppLaunch {
        logger.error(
            "Launch stopped at \(step.name): \(String(describing: error))"
        )
        return .failed(step, String(describing: error))
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
