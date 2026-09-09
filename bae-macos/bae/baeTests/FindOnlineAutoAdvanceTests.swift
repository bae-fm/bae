import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Find online returning to the draft on its own when identification writes
/// the pick.
///
/// The run matches one pressing and core commits it without asking, so the
/// candidate's draft comes to say identification wrote it. There is then
/// nothing left to do on this pane — clicking the row would apply what is
/// already applied — so the pane leaves the way a pick by hand leaves it.
///
/// Driven the way the app drives it: the pane is rebuilt from a new candidate
/// value, and the rule reads the transition between the two.
@MainActor
@Suite("Find online advances on identification's pick")
struct FindOnlineAutoAdvanceTests {
    /// The author and the draft are written as one row, so the person watching
    /// the spinner sees the pick land and the pane go back to it.
    @Test("the draft coming to say identification wrote it returns the pane")
    func identificationsPickReturnsToTheDraft() async {
        let back = BackRecorder()
        let (window, host) = AutoAdvanceHosting.host(
            pane(candidate: .awaitingAnAuthor, onBack: back.record)
        )
        await SnapshotTestSupport.settle(host)
        #expect(back.count == 0)

        host.rootView = pane(
            candidate: .writtenByIdentification,
            onBack: back.record
        )
        await SnapshotTestSupport.settle(host)

        #expect(back.count == 1)
        withExtendedLifetime(window) {}
    }

    /// Opening Find online on a candidate identification already picked for is
    /// a deliberate visit — the person wants to look at the other pressings,
    /// or search. The pane must not bounce them out of it.
    @Test("a draft identification had already written keeps the pane open")
    func aDraftAlreadyWrittenByIdentificationStays() async {
        let back = BackRecorder()
        let (window, host) = AutoAdvanceHosting.host(
            pane(candidate: .writtenByIdentification, onBack: back.record)
        )
        await SnapshotTestSupport.settle(host)

        host.rootView = pane(
            candidate: .writtenByIdentification,
            onBack: back.record
        )
        await SnapshotTestSupport.settle(host)

        #expect(back.count == 0)
        withExtendedLifetime(window) {}
    }

    /// A pick the person made owns its own way out: the store writes the pane
    /// back to the draft when the read lands. Nobody but them wrote this
    /// draft, so this rule stays out of it and the pane is not sent back twice.
    @Test("a pick the person made is not this rule's to act on")
    func aPickThePersonMadeIsNotActedOn() async {
        let back = BackRecorder()
        let (window, host) = AutoAdvanceHosting.host(
            pane(candidate: .awaitingAnAuthor, onBack: back.record)
        )
        await SnapshotTestSupport.settle(host)

        host.rootView = pane(candidate: .writtenByTheUser, onBack: back.record)
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

// MARK: - Fixtures

@MainActor
extension Candidate {
    /// The run matched one pressing and nothing has claimed the draft yet.
    static var awaitingAnAuthor: Candidate {
        soleMatch(pick: nil, author: .nobody)
    }

    /// Identification committed the pick: one row write, so the draft and its
    /// author changed together.
    static var writtenByIdentification: Candidate {
        soleMatch(
            pick: PreviewData.exactPressings[1].pick,
            author: .identification
        )
    }

    /// The person picked the same pressing themselves.
    static var writtenByTheUser: Candidate {
        soleMatch(pick: PreviewData.exactPressings[1].pick, author: .user)
    }

    private static func soleMatch(
        pick: BridgeMetadataProvenance?,
        author: BridgeMetadataAuthor
    ) -> Candidate {
        var candidate = Candidate(
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: pick,
                metadataAuthor: author
            )
        )
        // One pressing, so the pane draws the answer a sole match settles on.
        candidate.resumedIdentifyState =
            PreviewData.searchStateFinalizing.identifyState
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
