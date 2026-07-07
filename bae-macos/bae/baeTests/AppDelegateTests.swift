import Testing

@testable import bae

/// Covers the `AppDelegate` transitions that don't require a live library.
/// Opening or forgetting a library goes through the global `initApp` /
/// `forgetLibrary` bridge on a real opened core, which a unit test can't stand
/// up; the removal semantics — deleting the data directory, active pointer, and
/// key, and the fail-loud behavior — are covered by bae-core's library-manager
/// lifecycle tests.
@MainActor
@Suite("AppDelegate")
struct AppDelegateTests {

    @Test("forgetActiveLibrary with no open library is a no-op")
    func forgetWithoutLibraryIsNoOp() {
        let delegate = AppDelegate()
        #expect(delegate.appService == nil)

        delegate.forgetActiveLibrary()

        guard case .loading = delegate.screen else {
            Issue.record("screen should stay .loading with no library open")
            return
        }
    }
}
