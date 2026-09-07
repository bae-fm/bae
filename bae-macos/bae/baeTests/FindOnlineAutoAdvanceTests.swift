import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Find online returning to the draft on its own when core picks a sole match.
///
/// The run matches one pressing, core commits it without asking, and the
/// candidate's draft comes to carry that pick. There is then nothing left to
/// do on this pane — clicking the row would apply what is already applied — so
/// the pane leaves the way a pick by hand leaves it.
///
/// Driven the way the app drives it: the pane is rebuilt from a new candidate
/// value, and the rule reads the transition between the two.
@MainActor
@Suite("Find online advances on core's own pick")
struct FindOnlineAutoAdvanceTests {
    /// The verdict and the draft are written as one row, so the person watching
    /// the spinner sees the pick land and the pane go back to it.
    @Test("core's pick landing on the draft returns the pane to it")
    func corePickLandingReturnsToTheDraft() async {
        let back = BackRecorder()
        let (window, host) = AutoAdvanceHosting.host(
            pane(candidate: .committingTheSoleMatch, onBack: back.record)
        )
        await SnapshotTestSupport.settle(host)
        #expect(back.count == 0)

        host.rootView = pane(
            candidate: .settledOnTheSoleMatch,
            onBack: back.record
        )
        await SnapshotTestSupport.settle(host)

        #expect(back.count == 1)
        withExtendedLifetime(window) {}
    }

    /// Opening Find online on a candidate core already picked for is a
    /// deliberate visit — the person wants to look at the other pressings, or
    /// search. The pane must not bounce them out of it, even as the run's
    /// placement settles underneath.
    @Test("a draft that already carried the pick keeps the pane open")
    func aDraftThatAlreadyCarriedThePickStays() async {
        let back = BackRecorder()
        let (window, host) = AutoAdvanceHosting.host(
            pane(
                candidate: .committingAPickTheDraftAlreadyCarries,
                onBack: back.record
            )
        )
        await SnapshotTestSupport.settle(host)

        host.rootView = pane(
            candidate: .settledOnTheSoleMatch,
            onBack: back.record
        )
        await SnapshotTestSupport.settle(host)

        #expect(back.count == 0)
        withExtendedLifetime(window) {}
    }

    /// A pick the person made owns its own way out — `applyMetadata`'s
    /// `onConfirmed`. Nothing was being committed here, so this rule stays out
    /// of it and the pane is not sent back twice.
    @Test("a pick nobody was committing is not this rule's to act on")
    func aPickNobodyWasCommittingIsNotActedOn() async {
        let back = BackRecorder()
        let (window, host) = AutoAdvanceHosting.host(
            pane(candidate: .offeringTheSoleMatch, onBack: back.record)
        )
        await SnapshotTestSupport.settle(host)

        host.rootView = pane(
            candidate: .settledOnTheSoleMatch,
            onBack: back.record
        )
        await SnapshotTestSupport.settle(host)

        #expect(back.count == 0)
        withExtendedLifetime(window) {}
    }

    private func pane(
        candidate: Candidate,
        onBack: @escaping () -> Void
    ) -> some View {
        ImportSearchFlow.buildSearchPane(
            services: ImportSearchFlow.ImportServices(
                importer: Importer.stub(),
                importStore: ImportStore()
            ),
            input: ImportSearchFlow.SearchPaneInput(
                candidate: candidate,
                key: candidate.key,
                selectedReleaseId: nil,
                runtime: nil,
                liveSignals: nil
            ),
            openSettings: {},
            onBack: onBack,
            onSelect: { _ in }
        )
        .frame(
            width: AutoAdvanceHosting.size.width,
            height: AutoAdvanceHosting.size.height
        )
        .importPreviewEnvironment()
    }
}

/// The rule on its own, without a view: what the pane compares between two
/// readings of the same candidate.
@MainActor
@Suite("What counts as core's pick landing")
struct SoleMatchProgressTests {
    private let pick = PreviewData.exactPressings[1].pick

    @Test("the pick core was committing becoming the draft's is the landing")
    func theLanding() {
        #expect(
            progress(committing: nil, applied: pick)
                .followedCorePick(after: progress(committing: pick))
        )
    }

    @Test("a draft that already carried it has nothing to land")
    func alreadyCarried() {
        #expect(
            !progress(committing: nil, applied: pick)
                .followedCorePick(
                    after: progress(committing: pick, applied: pick)
                )
        )
    }

    @Test("nothing was being committed, so nothing landed")
    func nothingCommitting() {
        #expect(
            !progress(committing: nil, applied: pick)
                .followedCorePick(after: progress())
        )
    }

    @Test("a draft that came to carry some other pick did not land this one")
    func someOtherPick() {
        #expect(
            !progress(
                committing: nil,
                applied: PreviewData.exactPressings[0].pick
            )
            .followedCorePick(after: progress(committing: pick))
        )
    }

    /// Clearing the draft while core commits is not a landing either.
    @Test("a draft cleared under a run has landed nothing")
    func clearedDraft() {
        #expect(
            !progress().followedCorePick(after: progress(committing: pick))
        )
    }

    private func progress(
        committing: BridgeMetadataProvenance? = nil,
        applied: BridgeMetadataProvenance? = nil
    ) -> ImportSearchFlow.SoleMatchProgress {
        ImportSearchFlow.SoleMatchProgress(
            committing: committing,
            applied: applied
        )
    }
}

// MARK: - Fixtures

@MainActor
extension Candidate {
    /// The run matched one pressing and core is committing it: the row is
    /// finalizing and the draft has not been written yet.
    static var committingTheSoleMatch: Candidate {
        soleMatch(placement: .identification(status: .finalizing), pick: nil)
    }

    /// Core committed the pick and the row settled: one row write, so both
    /// changed together.
    static var settledOnTheSoleMatch: Candidate {
        soleMatch(placement: .ready, pick: PreviewData.exactPressings[1].pick)
    }

    /// Find online opened on a candidate whose draft core had already picked
    /// for, while its row still reads as finalizing.
    static var committingAPickTheDraftAlreadyCarries: Candidate {
        soleMatch(
            placement: .identification(status: .finalizing),
            pick: PreviewData.exactPressings[1].pick
        )
    }

    /// The sole match is on offer and nothing is being committed — the state a
    /// pick by hand starts from.
    static var offeringTheSoleMatch: Candidate {
        soleMatch(placement: .ready, pick: nil)
    }

    private static func soleMatch(
        placement: BridgeTriagePlacement,
        pick: BridgeMetadataProvenance?
    ) -> Candidate {
        var candidate = Candidate(
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: pick
            )
        )
        // One pressing, so the pane reads it as the one core picks on its own.
        candidate.resumedIdentifyState =
            PreviewData.searchStateFinalizing.identifyState
        candidate.row?.placement = placement
        return candidate
    }
}

@MainActor
private final class BackRecorder {
    private(set) var count = 0

    func record() {
        count += 1
    }
}

/// Hosts without making the window key: key status is process-wide, and these
/// run alongside tests that type into a field and expect to keep the focus.
@MainActor
private enum AutoAdvanceHosting {
    static let size = NSSize(width: 900, height: 620)

    static func host<V: View>(
        _ view: V
    ) -> (window: NSWindow, host: NSHostingView<V>) {
        let bounds = NSRect(origin: .zero, size: size)
        let host = NSHostingView(rootView: view)
        host.frame = bounds
        let window = NSWindow(
            contentRect: bounds,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = host
        return (window, host)
    }
}
