import AppKit
import BaeKit
import Testing

@testable import bae

/// Covers the `AppDelegate` transitions that don't require a live library.
/// Opening or forgetting a library goes through the global `initApp` bridge
/// and a real opened core's `close`, which a unit test can't stand up; the
/// removal semantics — deleting the data directory, active pointer, and every
/// keyring entry once the library is closed, and refusing while it is open —
/// are covered by bae-core's and bae-bridge's lifecycle tests.
@MainActor
@Suite("AppDelegate")
struct AppDelegateTests {

    @Test("already regular application keeps its activation policy")
    func alreadyRegularApplicationKeepsPolicy() {
        let application = FakeActivationPolicyApplication(
            policy: .regular,
            transitionResult: false
        )

        AppDelegate.setRegularActivationPolicyIfNeeded(application)

        #expect(application.requestedPolicies.isEmpty)
    }

    @Test("hidden application adopts the regular activation policy")
    func hiddenApplicationAdoptsRegularPolicy() {
        let application = FakeActivationPolicyApplication(
            policy: .accessory,
            transitionResult: true
        )

        AppDelegate.setRegularActivationPolicyIfNeeded(application)

        #expect(application.requestedPolicies == [.regular])
    }

    @Test("preview host does not construct application services")
    func previewHostIsInert() {
        var serviceConstructionCount = 0
        let application = FakeActivationPolicyApplication(
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
        let application = FakeActivationPolicyApplication(
            policy: .accessory,
            transitionResult: true
        )
        let delegate = AppDelegate(runtime: .testHost)

        delegate.prepareApplicationForLaunch(application)

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

    /// An open that stopped on a reset or restore left partway lands on the
    /// one call that finishes it, never the other.
    @Test("an unfinished replacement opens the call that finishes it")
    func unfinishedReplacementLandsOnItsCall() {
        let restore = AppScreen.recovering(
            libraryId: "lib-01",
            from: BridgeError.Diagnostic(
                category: .replacementUnfinished(by: .restore),
                detail: "restore_device"
            )
        )
        guard case .restoreUnfinished(let restoring) = restore else {
            Issue.record(
                "an unfinished restore lands on \(String(describing: restore))"
            )
            return
        }
        #expect(restoring == "lib-01")

        let reset = AppScreen.recovering(
            libraryId: "lib-02",
            from: BridgeError.Diagnostic(
                category: .replacementUnfinished(by: .reset),
                detail: "reset_device"
            )
        )
        guard case .resetUnfinished(let resetting) = reset else {
            Issue.record(
                "an unfinished reset lands on \(String(describing: reset))"
            )
            return
        }
        #expect(resetting == "lib-02")
    }

    @Test("a failed open with no recovery here lands on none")
    func otherOpenFailuresHaveNoRecovery() {
        let damaged = AppScreen.recovering(
            libraryId: "lib-01",
            from: BridgeError.Diagnostic(
                category: .deviceResetRequired,
                detail: "damaged database"
            )
        )
        guard case .resetRequired = damaged else {
            Issue.record(
                "a damaged database lands on \(String(describing: damaged))"
            )
            return
        }
        #expect(
            AppScreen.recovering(
                libraryId: "lib-01",
                from: BridgeError.Diagnostic(
                    category: .network,
                    detail: "offline"
                )
            ) == nil
        )
    }

    /// Finishing from the welcome's list, then leaving, returns to the
    /// welcome while no library is open.
    @Test(
        "leaving a finishing screen with no library open returns to the welcome"
    )
    func leavingFinishingReturnsToWelcome() {
        let delegate = AppDelegate()

        delegate.presentFinishing(libraryId: "lib-01", by: .restore)
        guard case .restoreUnfinished("lib-01") = delegate.screen else {
            Issue.record("finishing a restore shows \(delegate.screen)")
            return
        }
        delegate.leaveRecovery()

        guard case .welcome = delegate.screen else {
            Issue.record("leaving shows \(delegate.screen)")
            return
        }
    }
}

@MainActor
private final class FakeActivationPolicyApplication:
    ApplicationActivationPolicy
{
    private let policy: NSApplication.ActivationPolicy
    private let transitionResult: Bool
    private(set) var requestedPolicies: [NSApplication.ActivationPolicy] = []

    init(
        policy: NSApplication.ActivationPolicy,
        transitionResult: Bool
    ) {
        self.policy = policy
        self.transitionResult = transitionResult
    }

    func activationPolicy() -> NSApplication.ActivationPolicy {
        policy
    }

    func setActivationPolicy(
        _ activationPolicy: NSApplication.ActivationPolicy
    ) -> Bool {
        requestedPolicies.append(activationPolicy)
        return transitionResult
    }
}
