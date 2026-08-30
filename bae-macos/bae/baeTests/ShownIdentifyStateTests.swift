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
            import: nil
        )
    }

    @Test("a live run outranks the stored verdict's resumed state")
    func liveRunWins() {
        let shown = shownIdentifyState(
            resumed: .notFoundAnywhere,
            runtime: runtime(
                .triangulating(discid: .computing, barcode: .scanning)
            )
        )
        #expect(shown == .triangulating(discid: .computing, barcode: .scanning))
    }

    @Test("nothing running leaves the resumed state")
    func nothingRunning() {
        #expect(
            shownIdentifyState(resumed: .notFoundAnywhere, runtime: nil)
                == .notFoundAnywhere
        )
    }

    @Test("a run that is idle leaves the resumed state")
    func idleRunDefersToTheVerdict() {
        #expect(
            shownIdentifyState(
                resumed: .notFoundAnywhere,
                runtime: runtime(.idle)
            ) == .notFoundAnywhere
        )
    }
}
