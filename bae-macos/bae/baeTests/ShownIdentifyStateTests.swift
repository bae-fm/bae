import BaeKit
import Testing

@testable import bae

/// The one rule about which identify state a surface shows. It used to live on
/// `Candidate`, reconciling two stored fields; the run in flight is not stored
/// any more, so the rule is a function of the two values a surface holds.
@Suite("The identify state a candidate shows")
struct ShownIdentifyStateTests {
    private func runtime(
        _ identification: BridgeIdentificationInFlight?
    ) -> BridgeCandidateRuntimeSnapshot {
        BridgeCandidateRuntimeSnapshot(
            identification: identification,
            import: nil,
            search: nil
        )
    }

    @Test("a live run outranks the stored verdict's resumed state")
    func liveRunWins() {
        let shown = shownIdentifyState(
            resumed: .notFoundAnywhere(run: nil),
            runtime: runtime(
                .run(
                    state: .triangulating(
                        run: PreviewData.identifyRunStarting,
                        groups: [],
                        libraryStatuses: [:],
                        agreements: [:],
                        narrowedOutCount: 0
                    )
                )
            )
        )
        #expect(
            shown
                == .triangulating(
                    run: PreviewData.identifyRunStarting,
                    groups: [],
                    libraryStatuses: [:],
                    agreements: [:],
                    narrowedOutCount: 0
                )
        )
    }

    @Test("a wait on the identification queue outranks the resumed state")
    func queuedWins() {
        #expect(
            shownIdentifyState(
                resumed: .notFoundAnywhere(run: nil),
                runtime: runtime(.queued)
            ) == .queued
        )
    }

    @Test("nothing running leaves the resumed state")
    func nothingRunning() {
        #expect(
            shownIdentifyState(
                resumed: .notFoundAnywhere(run: nil),
                runtime: nil
            ) == .notFoundAnywhere(run: nil)
        )
    }

    @Test("a runtime with no identification leaves the resumed state")
    func noIdentificationDefersToTheVerdict() {
        #expect(
            shownIdentifyState(
                resumed: .notFoundAnywhere(run: nil),
                runtime: runtime(nil)
            ) == .notFoundAnywhere(run: nil)
        )
    }
}
