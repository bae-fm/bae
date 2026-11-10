import BaeKit
import Testing

@testable import bae

@Suite("ActivityProblems")
struct ActivityProblemsTests {
    @Test("problems list in the panel's order")
    func listedOrder() {
        let problems = ActivityProblems(
            app: [signIn, libraryLoad],
            activity: activity(failure: failure),
            error: unread,
            dismissed: []
        )
        #expect(
            problems.listed == [
                .signInSettings, .libraryLoad, .syncStatusUnread, .syncFailure,
            ]
        )
        #expect(problems.shown == problems.listed)
    }

    @Test("a dismissed problem leaves the shown ones and the rest stay")
    func dismissedLeavesShown() {
        let problems = ActivityProblems(
            app: [signIn, libraryLoad],
            activity: activity(failure: failure),
            error: unread,
            dismissed: [.libraryLoad]
        )
        #expect(problems.listed.count == 4)
        #expect(
            problems.shown == [
                .signInSettings, .syncStatusUnread, .syncFailure,
            ]
        )
    }

    @Test("nothing needs attention once every problem is dismissed")
    func everyProblemDismissed() {
        let problems = ActivityProblems(
            app: [signIn],
            activity: activity(failure: failure),
            error: nil,
            dismissed: [.signInSettings, .syncFailure]
        )
        #expect(!problems.needsAttention)
    }

    @Test("a listed record needs attention however much is dismissed")
    func recordKeepsAttention() {
        let problems = ActivityProblems(
            app: [signIn],
            activity: .preview(needsAttention: true),
            error: nil,
            dismissed: [.signInSettings]
        )
        #expect(problems.recordsListed)
        #expect(problems.needsAttention)
    }

    @Test("a new kind of problem needs attention after the rest are dismissed")
    func newKindNeedsAttention() {
        let dismissed: Set<ActivityProblemID> = [.signInSettings]
        #expect(
            !ActivityProblems(
                app: [signIn],
                activity: nil,
                error: nil,
                dismissed: dismissed
            )
            .needsAttention
        )
        #expect(
            ActivityProblems(
                app: [signIn],
                activity: nil,
                error: unread,
                dismissed: dismissed
            )
            .needsAttention
        )
    }

    private let failure = BridgeError.Diagnostic(
        category: .internal,
        detail: "dns error"
    )

    private let unread = DisplayError(line: "Couldn't read")

    private var signIn: AppProblem {
        AppProblem(
            id: .signInSettings,
            title: "Sign-in settings",
            advice: nil,
            error: unread
        )
    }

    private var libraryLoad: AppProblem {
        AppProblem(
            id: .libraryLoad,
            title: "Library failed to open",
            advice: nil,
            error: unread
        )
    }

    private func activity(failure: BridgeError?) -> BridgeSyncActivity {
        var activity = BridgeSyncActivity.preview(needsAttention: false)
        activity.failure = failure
        return activity
    }
}
