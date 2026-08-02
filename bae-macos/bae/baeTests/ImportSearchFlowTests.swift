import BaeKit
import Testing

@testable import bae

/// `readyAutoPick` decides when a selected candidate's pane opens on its
/// row's settled match without a click. Its guards are what keep the seed
/// from firing where a pick would be wrong: rows outside Ready, folders whose
/// identity is already settled, and prefetches that already failed once.
@MainActor
struct ImportSearchFlowReadyAutoPickTests {
    private var candidate: Candidate {
        PreviewData.folderCandidates[0]
    }

    private func readyRow(for candidate: Candidate) -> BridgeTriageRow {
        PreviewData.triageRow(
            for: candidate,
            placement: .ready,
            matched: PreviewData.triageMatch(
                releaseId: "rel-ready",
                title: "Album Title"
            ),
            selectable: true
        )
    }

    @Test func seedsFromAReadyRow() {
        let pick = ImportSearchFlow.readyAutoPick(
            candidate: candidate,
            row: readyRow(for: candidate)
        )
        #expect(pick?.releaseId == "rel-ready")
    }

    @Test func ignoresRowsOutsideReady() {
        // Done and Skipped rows carry `matched` too, and a candidate rebuilt
        // at launch has nothing settled again — placement is the only thing
        // standing between an imported row and a re-opened confirm pane.
        for placement: BridgeTriagePlacement in [.done, .skipped] {
            let row = PreviewData.triageRow(
                for: candidate,
                placement: placement,
                matched: PreviewData.triageMatch(
                    releaseId: "rel-done",
                    title: "Album Title"
                ),
                selectable: false
            )
            #expect(
                ImportSearchFlow.readyAutoPick(candidate: candidate, row: row)
                    == nil
            )
        }
    }

    @Test func ignoresAnUnsettledLead() {
        // Several matches: the row leads with one of them, but which pressing
        // is exactly the open question — nothing to seed.
        let row = PreviewData.triageRow(
            for: candidate,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(
                    disagreement: .severalMatches(count: 4)
                )
            ),
            matched: PreviewData.triageMatch(
                releaseId: "rel-lead",
                title: "Album Title"
            ),
            selectable: false
        )
        #expect(
            ImportSearchFlow.readyAutoPick(candidate: candidate, row: row)
                == nil
        )
    }

    @Test func yieldsToAPickAlreadyIn() {
        var picked = candidate
        picked.pick = CandidatePick(
            releaseId: "rel-user-chose",
            source: .musicBrainz
        )
        #expect(
            ImportSearchFlow.readyAutoPick(
                candidate: picked,
                row: readyRow(for: picked)
            ) == nil
        )
    }

    @Test func yieldsToAnIdentityAlreadySettled() {
        // A pick that resolved and a folder read as its own tags both settle
        // the identity choice; neither wants a Ready row seeded over it.
        let settled: [BridgeIdentityChoice] = [
            .exact(releaseId: "rel-picked", source: .musicBrainz),
            .unknown,
        ]
        for choice in settled {
            var advanced = candidate
            advanced.identityChoice = choice
            #expect(
                ImportSearchFlow.readyAutoPick(
                    candidate: advanced,
                    row: readyRow(for: advanced)
                ) == nil
            )
        }
    }

    @Test func staysDownAfterAFailedPrefetch() {
        // Failure clears what the pick had settled and sets `error`; without
        // this guard the seed would immediately retry the same prefetch, and
        // on a persistent failure, retry it forever.
        var failed = candidate
        failed.error = "Failed to load release details"
        #expect(
            ImportSearchFlow.readyAutoPick(
                candidate: failed,
                row: readyRow(for: failed)
            ) == nil
        )
    }

    @Test func absentRowSeedsNothing() {
        #expect(
            ImportSearchFlow.readyAutoPick(candidate: candidate, row: nil)
                == nil
        )
    }
}
