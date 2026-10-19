import AppKit
import BaeKit
import os.log

let baeAppLogger = Logger.bae("BaeApp")
let baeAppProcessEnvironment = ProcessInfo.processInfo.environment
let baeAppEdition: AppEdition = {
    #if BAE_OAUTH_PROVIDERS
        .bae
    #else
        .baeium
    #endif
}()

/// AppKit's entry: the application runs with `AppDelegate`, which owns
/// startup, the main menu and the windows.
@main
enum BaeApp {
    @MainActor
    static func main() {
        let delegate = AppDelegate()
        NSApplication.shared.delegate = delegate
        // The application holds its delegate weakly, and the run never
        // returns.
        withExtendedLifetime(delegate) {
            _ = NSApplicationMain(CommandLine.argc, CommandLine.unsafeArgv)
        }
    }
}
