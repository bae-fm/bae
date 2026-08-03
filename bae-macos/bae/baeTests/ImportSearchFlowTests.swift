import BaeKit
import Testing

@testable import bae

/// `pickedResume` decides when a selected candidate's pane opens on the
/// identity its row already carries — the settled single match, or the choice
/// the user made before a restart — without a click. Its guards are what keep
/// the resume from firing where it would be wrong: rows past deciding,
/// folders whose identity is already settled, and prefetches that already
/// failed once.
@MainActor
struct ImportSearchFlowPickedResumeTests {
    private var candidate: Candidate {
        PreviewData.folderCandidates[0]
    }

    private let releasePick = BridgeIdentityPick.release(
        source: .musicBrainz,
        releaseId: "rel-picked",
        claim: .exact
    )

    private func pickedRow(
        for candidate: Candidate,
        placement: BridgeTriagePlacement,
        picked: BridgeIdentityPick?
    ) -> BridgeTriageRow {
        PreviewData.triageRow(
            for: candidate,
            placement: placement,
            matched: PreviewData.triageMatch(
                releaseId: "rel-picked",
                title: "Album Title"
            ),
            selectable: false,
            picked: picked
        )
    }

    @Test
    func appliesAReadyRowsSettledPick() {
        // A settled single match is a pick identification made — the same
        // record a click writes, resumed the same way.
        let row = pickedRow(
            for: candidate,
            placement: .ready,
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == releasePick
        )
    }

    @Test
    func appliesAChoiceMadeOnANeedsYouRow() {
        let row = pickedRow(
            for: candidate,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(
                    disagreement: .severalMatches(count: 4)
                )
            ),
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == releasePick
        )
    }

    @Test
    func appliesAStoredUnknownChoice() {
        let row = pickedRow(
            for: candidate,
            placement: .needsYou(
                group: .noMatch,
                reason: .disagreement(disagreement: .noMatch)
            ),
            picked: .unknown
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == .unknown
        )
    }

    @Test
    func aRowWithNothingDecidedResumesNothing() {
        let row = pickedRow(
            for: candidate,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(
                    disagreement: .severalMatches(count: 4)
                )
            ),
            picked: nil
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                == nil
        )
    }

    @Test
    func ignoresRowsPastDeciding() {
        // Done and Skipped rows keep their pick too, and a candidate rebuilt
        // at launch starts back with nothing settled — placement is the only
        // thing standing between an imported row and a re-opened commit-able
        // pane.
        for placement: BridgeTriagePlacement in [.done, .skipped] {
            let row = pickedRow(
                for: candidate,
                placement: placement,
                picked: releasePick
            )
            #expect(
                ImportSearchFlow.pickedResume(candidate: candidate, row: row)
                    == nil
            )
        }
    }

    @Test
    func yieldsToAPickAlreadyIn() {
        var picked = candidate
        picked.pick = CandidatePick(
            releaseId: "rel-user-chose",
            source: .musicBrainz,
            claim: .exact
        )
        let row = pickedRow(
            for: picked,
            placement: .ready,
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: picked, row: row) == nil
        )
    }

    @Test
    func yieldsToAnIdentityAlreadySettled() {
        // A pick that resolved and a folder read as its own tags both settle
        // the identity choice; neither wants the stored pick re-applied over
        // it.
        let settled: [BridgeIdentityChoice] = [
            .exact(releaseId: "rel-picked", source: .musicBrainz),
            .unknown,
        ]
        for choice in settled {
            var advanced = candidate
            advanced.identityChoice = choice
            let row = pickedRow(
                for: advanced,
                placement: .ready,
                picked: releasePick
            )
            #expect(
                ImportSearchFlow.pickedResume(candidate: advanced, row: row)
                    == nil
            )
        }
    }

    @Test
    func staysDownAfterAFailedPrefetch() {
        // Failure clears what the pick had settled and sets `error`; without
        // this guard the resume would immediately retry the same prefetch,
        // and on a persistent failure, retry it forever.
        var failed = candidate
        failed.error = "Failed to load release details"
        let row = pickedRow(
            for: failed,
            placement: .ready,
            picked: releasePick
        )
        #expect(
            ImportSearchFlow.pickedResume(candidate: failed, row: row) == nil
        )
    }

    @Test
    func absentRowResumesNothing() {
        #expect(
            ImportSearchFlow.pickedResume(candidate: candidate, row: nil)
                == nil
        )
    }
}
