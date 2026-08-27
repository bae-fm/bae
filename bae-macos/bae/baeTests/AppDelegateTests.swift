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

    @Test("preview host does not construct application services")
    func previewHostIsInert() {
        var serviceConstructionCount = 0
        let runtime = AppRuntime(
            environment: ["XCODE_RUNNING_FOR_PREVIEWS": "1"]
        )

        let delegate = AppDelegate(
            runtime: runtime,
            makeApplicationServices: {
                serviceConstructionCount += 1
                return ApplicationServices()
            }
        )

        #expect(runtime == .preview)
        #expect(serviceConstructionCount == 0)
        #expect(delegate.applicationServices == nil)
    }

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
