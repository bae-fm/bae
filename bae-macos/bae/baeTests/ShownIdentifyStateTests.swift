import BaeKit
import Testing

@testable import bae

/// The one rule about which identify state a surface shows. It used to live on
/// `Candidate`, reconciling two stored fields; the run in flight is not stored
/// any more, so the rule is a function of the two values a surface holds.
@Suite("The identify state a candidate shows")
struct ShownIdentifyStateTests {
    private func runtime(
        _ state: BridgeIdentifyState
    ) -> BridgeCandidateRuntimeSnapshot {
        BridgeCandidateRuntimeSnapshot(
            identifyState: state,
            signalsToolbar: BridgeSignalsToolbar(signals: []),
            import: nil,
            search: nil
        )
    }

    @Test("a live run outranks the stored verdict's resumed state")
    func liveRunWins() {
        let shown = shownIdentifyState(
            resumed: .notFoundAnywhere(run: nil),
            runtime: runtime(
                .triangulating(
                    run: PreviewData.identifyRunStarting,
                    groups: [],
                    libraryStatuses: [:],
                    agreements: [:],
                    narrowedOut: BridgeNarrowedOut(
                        groups: [],
                        libraryStatuses: [:],
                        agreements: [:]
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
                    narrowedOut: .nothing
                )
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

    @Test("a run that is idle leaves the resumed state")
    func idleRunDefersToTheVerdict() {
        #expect(
            shownIdentifyState(
                resumed: .notFoundAnywhere(run: nil),
                runtime: runtime(.idle)
            ) == .notFoundAnywhere(run: nil)
        )
    }
}
