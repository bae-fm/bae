import SwiftUI

/// The process's entry. What has to hold before any framework reads app state
/// is set up here, then the app runs.
@main
enum BaeMain {
    @MainActor
    static func main() {
        #if DEBUG
            UITestDefaults.install()
        #endif
        BaeApp.main()
    }
}
