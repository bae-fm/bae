import AppKit
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

    @Test("already regular application keeps its activation policy")
    func alreadyRegularApplicationKeepsPolicy() {
        let application = ActivationPolicyApplication(
            policy: .regular,
            transitionResult: false
        )

        AppDelegate.setRegularActivationPolicyIfNeeded(application)

        #expect(application.requestedPolicies.isEmpty)
    }

    @Test("hidden application adopts the regular activation policy")
    func hiddenApplicationAdoptsRegularPolicy() {
        let application = ActivationPolicyApplication(
            policy: .accessory,
            transitionResult: true
        )

        AppDelegate.setRegularActivationPolicyIfNeeded(application)

        #expect(application.requestedPolicies == [.regular])
    }

    @Test("preview host does not construct application services")
    func previewHostIsInert() {
        var serviceConstructionCount = 0
        let application = ActivationPolicyApplication(
            policy: .accessory,
            transitionResult: true
        )
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
        delegate.applicationWillFinishLaunching(
            Notification(
                name: NSApplication.willFinishLaunchingNotification,
                object: application
            )
        )

        #expect(runtime == .preview)
        #expect(serviceConstructionCount == 0)
        #expect(delegate.applicationServices == nil)
        #expect(application.requestedPolicies.isEmpty)
    }

    @Test("unit-test host adopts the regular activation policy")
    func testHostAdoptsRegularPolicy() {
        let application = ActivationPolicyApplication(
            policy: .accessory,
            transitionResult: true
        )
        let delegate = AppDelegate(runtime: .testHost)

        delegate.applicationWillFinishLaunching(
            Notification(
                name: NSApplication.willFinishLaunchingNotification,
                object: application
            )
        )

        #expect(application.requestedPolicies == [.regular])
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

@MainActor
private final class ActivationPolicyApplication: NSApplication {
    private let policy: NSApplication.ActivationPolicy
    private let transitionResult: Bool
    private(set) var requestedPolicies: [NSApplication.ActivationPolicy] = []

    init(
        policy: NSApplication.ActivationPolicy,
        transitionResult: Bool
    ) {
        self.policy = policy
        self.transitionResult = transitionResult
        super.init()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("ActivationPolicyApplication does not support decoding")
    }

    override func activationPolicy() -> NSApplication.ActivationPolicy {
        policy
    }

    override func setActivationPolicy(
        _ activationPolicy: NSApplication.ActivationPolicy
    ) -> Bool {
        requestedPolicies.append(activationPolicy)
        return transitionResult
    }
}
