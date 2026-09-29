import AppKit
import BaeKit
import SwiftUI
import Testing
import XCTest

@testable import bae

private struct ExternalMetadataApplication: Equatable {
    let key: String
    let link: BridgeReleaseLink
}

@MainActor
private final class MetadataSourceRecorder {
    var externalApplications: [ExternalMetadataApplication] = []
    var fileTagApplications: [String] = []
    var resetKeys: [String] = []
    var resetError: (any Error)?
    var events: [String] = []
    var clearedKeys: [String] = []
    var identifiedKeys: [String] = []
    var automaticKeys: [String] = []
    var errors: [String] = []

    var importer: Importer {
        Importer(
            applyCandidateExternalMetadata: { [self] key, link in
                await MainActor.run {
                    externalApplications.append(
                        ExternalMetadataApplication(
                            key: key,
                            link: link
                        )
                    )
                }
                return .done
            },
            applyCandidateFileMetadata: { [self] key in
                await MainActor.run {
                    fileTagApplications.append(key)
                    return .done
                }
            },
            resetCandidateSetup: { [self] key in
                try await MainActor.run {
                    events.append("reset")
                    resetKeys.append(key)
                    if let resetError { throw resetError }
                }
            },
            clearCandidateMetadata: { [self] key in
                await MainActor.run { clearedKeys.append(key) }
                return 1
            },
            // Core's re-run is fire-and-forget from any isolation; every
            // press in these tests comes from the main actor, where this
            // recorder lives.
            rerunIdentifyForCandidate: { [self] key in
                MainActor.assumeIsolated { identifiedKeys.append(key) }
            },
            identifyAutomatically: { [self] key in
                await MainActor.run { automaticKeys.append(key) }
            }
        )
    }

    func services(_ store: ImportStore) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: store,
            endEditing: { [self] in events.append("end editing") },
            previewAudio: PreviewAudio.stub(),
            openDocument: { _, _ in },
            openImages: { _, _ in },
            onError: { [self] in errors.append($0) }
        )
    }
}

@MainActor
@Suite("Import metadata sources")
struct ImportMetadataSourceTests {}

extension ImportMetadataSourceTests {
    @Test("Reset finishes editing then resets the complete import setup")
    func resetUsesTheCanonicalOperation() async throws {
        let store = MappingFixtures.store(mapping: nil)
        let recorder = MetadataSourceRecorder()
        ImportMappingFlow.reset(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        try await Wait.until { !recorder.resetKeys.isEmpty }

        #expect(recorder.resetKeys == [MappingFixtures.candidateKey])
        #expect(recorder.events == ["end editing", "reset"])
        #expect(recorder.fileTagApplications.isEmpty)
        #expect(recorder.clearedKeys.isEmpty)
        #expect(recorder.errors.isEmpty)
    }

    @Test("Reset failures reach the existing error presentation")
    func resetReportsFailure() async throws {
        let recorder = MetadataSourceRecorder()
        recorder.resetError = NSError(
            domain: "ResetTest",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: "Source changed"]
        )
        ImportMappingFlow.reset(
            key: MappingFixtures.candidateKey,
            services: recorder.services(MappingFixtures.store(mapping: nil))
        )
        try await Wait.until { !recorder.errors.isEmpty }

        #expect(
            recorder.errors == ["Couldn't save that change: Source changed"]
        )
        #expect(recorder.fileTagApplications.isEmpty)
        #expect(recorder.clearedKeys.isEmpty)
    }

}

extension ImportMetadataSourceTests {
    /// Choosing a surface is written to core, not kept in the pane: the
    /// store records the write, and the next detail is what the pane shows.
    @Test("moving the pane asks core, and the detail shows where it went")
    func choosingASurfaceWritesItThrough() async throws {
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit,
        )
        let writes = SessionWriteRecorder()
        store.sessionWriter = .recording { writes.record($0) }

        store.movePane(.findOnline, forKey: MappingFixtures.candidateKey)
        try await Wait.until {
            !writes.paneMoves(forKey: MappingFixtures.candidateKey).isEmpty
        }

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                releaseLink: nil,
                presentation: .findOnline
            )
        )

        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .findOnline
        )
        #expect(
            writes.paneMoves(forKey: MappingFixtures.candidateKey)
                == [.findOnline]
        )
    }

    /// Resetting to tags is one command: it replaces the draft with what the
    /// candidate's own files say and leaves the pane on the draft it wrote.
    /// There is no surface to review the tags on first.
    @Test(
        "resetting to file metadata applies the folder's own metadata to the draft"
    )
    func resetToFileMetadataAppliesTheFoldersOwnMetadata() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()

        ImportMappingFlow.resetToFileMetadata(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        try await Wait.until {
            recorder.fileTagApplications == [MappingFixtures.candidateKey]
        }

        #expect(recorder.fileTagApplications == [MappingFixtures.candidateKey])
        #expect(recorder.errors.isEmpty)
    }

    @Test("applying an online result leaves the move to the draft to core")
    func onlineApplicationStoresTheDraft() async throws {
        let key = MappingFixtures.candidateKey
        let writes = SessionWriteRecorder()
        // The pane is on Find online, as the candidate's stored session says.
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit,
            presentation: .findOnline
        )
        store.sessionWriter = .recording { writes.record($0) }
        let recorder = MetadataSourceRecorder()

        ImportSearchFlow.applyMetadata(
            importer: recorder.importer,
            importStore: store,
            endEditing: {},
            key: key,
            application: .pick(MappingFixtures.link)
        )
        try await Wait.until {
            store.metadataApplicationSession(forKey: key) == nil
        }

        #expect(recorder.externalApplications.map(\.key) == [key])
        // Core moves the pane as the pick lands; the store asks for no move.
        #expect(writes.paneMoves(forKey: key).isEmpty)
        // What the pane shows is core's answer, so it stays on Find online
        // until the read the store just made comes back.
        #expect(
            store.candidate(forKey: key)?
                .metadataPresentation == .findOnline
        )

        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: MappingFixtures.provenance
            )
        )

        #expect(
            store.candidate(forKey: key)?.metadataPresentation == .draft
        )
    }

    /// Automatic asks core, which decides what it takes — the stored verdict
    /// shown as it stood, or a run started — and moves the pane itself: the
    /// store neither moves the pane nor asks for a run.
    @Test("Automatic asks core every time and decides nothing here")
    func automaticAsksCore() async throws {
        let writes = SessionWriteRecorder()
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit
        )
        store.sessionWriter = .recording { writes.record($0) }
        let recorder = MetadataSourceRecorder()
        let services = recorder.services(store)
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.identify(candidate, services: services)
        ImportMappingFlow.identify(candidate, services: services)
        try await Wait.until { recorder.automaticKeys.count == 2 }

        #expect(
            recorder.automaticKeys
                == [MappingFixtures.candidateKey, MappingFixtures.candidateKey]
        )
        #expect(recorder.identifiedKeys.isEmpty)
        #expect(writes.paneMoves(forKey: MappingFixtures.candidateKey).isEmpty)
    }

    /// Searching for a release opens the same page and asks for nothing: what
    /// it offers is the typed form, and a run is the other entry's to start.
    @Test("searching for a release opens the page and starts no run")
    func searchingForAReleaseStartsNoRun() async throws {
        let writes = SessionWriteRecorder()
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit
        )
        store.sessionWriter = .recording { writes.record($0) }
        let recorder = MetadataSourceRecorder()
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.movePane(
            .search,
            for: candidate,
            services: recorder.services(store)
        )
        try await Wait.until {
            !writes.paneMoves(forKey: MappingFixtures.candidateKey).isEmpty
        }

        #expect(recorder.identifiedKeys.isEmpty)
        #expect(recorder.automaticKeys.isEmpty)
        #expect(
            writes.paneMoves(forKey: MappingFixtures.candidateKey)
                == [.search]
        )
    }

    @Test("clearing metadata dispatches the candidate command")
    func clearMetadataDispatchesCommand() async throws {
        let store = MappingFixtures.store(mapping: nil)
        let recorder = MetadataSourceRecorder()

        ImportMappingFlow.clearMetadata(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        try await Wait.until { !recorder.clearedKeys.isEmpty }

        #expect(recorder.clearedKeys == [MappingFixtures.candidateKey])
        #expect(recorder.errors.isEmpty)
    }

}

/// What the draft card offers as ways to a release.
@MainActor
@Suite("The draft card's release entries")
struct ImportReleaseEntryTests {
    /// Two entries under one Identify label, named for what each one does
    /// rather than where it goes: both open the same page, and only the first
    /// asks for a run.
    @Test("the card names both ways to a release")
    func theCardNamesBothWaysToARelease() async throws {
        let lines = try await FindOnlineRendering.text(
            ImportReleaseHeader(
                releaseSummary: ImportReleaseSummary(
                    candidate: PreviewData.mappingCandidate,
                    editValues: PreviewData.confirmEditValues
                ),
                actionable: true,
                isReading: false,
                coverContent: nil,
                hasCoverOptions: true,
                editValues: PreviewData.confirmEditValues,
                records: [],
                editActions: ReleaseFieldWriter { _, _ in },
                editingCommands: EditingCommitCommands(),
                commit: nil,
                sourceActions: ImportReleaseSourceActions(
                    identifyAutomatically: {},
                    searchForRelease: {},
                    reset: {},
                    resetToFileMetadata: {},
                    clearMetadata: {},
                    unlink: {}
                ),
                localCoverSelections: [:],
                onEditCover: {},
                onSelectCover: { _ in }
            )
            .importPreviewEnvironment()
            .environment(Library.stub())
            .candidateReaderPreviewEnvironment(),
            size: NSSize(width: 900, height: 620),
            scale: 3
        )

        for label in [
            String(localized: "Identify"),
            String(localized: "Automatic"),
            String(localized: "Search"),
        ] {
            #expect(
                lines.contains { $0.localizedCaseInsensitiveContains(label) },
                "the card reads: \(lines)"
            )
        }
    }
}

@MainActor
final class ImportFileTagsRepeatabilityTests: XCTestCase {
    /// Resetting to tags twice reads the files twice and writes the draft
    /// twice: the command carries no memory of an earlier read, so nothing has
    /// to be cleared between them.
    func testTheDraftCanBeResetToTagsAgainWithoutClearingMetadata()
        async throws
    {
        let key = MappingFixtures.candidateKey
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()
        let services = recorder.services(store)

        for applications in 1...2 {
            ImportMappingFlow.resetToFileMetadata(key: key, services: services)
            try await Wait.until {
                recorder.fileTagApplications.count == applications
            }
            store.applyCandidateDetail(
                key: key,
                detail: MappingFixtures.detail(
                    mapping: MappingFixtures.fileTagsTable,
                    metadataProvenance: .fileMetadata,
                    metadataRevision: UInt64(applications)
                )
            )
            XCTAssertEqual(
                store.candidate(forKey: key)?.metadataPresentation,
                .draft
            )
        }

        XCTAssertEqual(recorder.fileTagApplications, [key, key])
        XCTAssertEqual(
            store.candidate(forKey: key)?.metadataProvenance,
            .fileMetadata
        )
    }

}

@MainActor
final class ImportMetadataCardLayoutTests: XCTestCase {
    func testSourceActionsLeadTheCardAboveCoverAndFields() async throws {
        NSApplication.shared.finishLaunching()
        let provenances: [BridgeMetadataProvenance?] = [
            nil,
            .fileMetadata,
        ]
        for provenance in provenances {
            try await assertCardLayout(provenance: provenance)
        }
    }

    /// A blank draft keeps its title and album year beside the cover and the
    /// release's text fields — year, one blank label row, barcode — below it.
    /// The label row's catalog number is hinted "Catalog #" rather than a
    /// dash, so it is told apart from the label name beside it.
    func testIdentityFieldsSitBesideTheCoverAndReleaseFieldsUnderIt()
        async throws
    {
        NSApplication.shared.finishLaunching()
        let recorder = MetadataCardActionRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: true,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 900)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            let fields = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .filter(\.isEditable)
            let cover = try coverFrame(in: host)
            XCTAssertEqual(fields.count, 6)
            for placeholder in [
                String(localized: "Album title"),
                String(localized: "Album year"),
            ] {
                let field = try XCTUnwrap(
                    fields.first { $0.placeholderString == placeholder }
                )
                let frame = field.convert(field.bounds, to: host)
                XCTAssertGreaterThanOrEqual(frame.minX, cover.maxX)
                XCTAssertLessThan(frame.minY, cover.maxY)
                XCTAssertGreaterThan(frame.maxY, cover.minY)
            }
            // The empty mark is an attributed placeholder: it carries its own
            // colour and the field's plain font.
            let dashed = fields.filter {
                $0.placeholderAttributedString?.string == "\u{2014}"
            }
            XCTAssertEqual(dashed.count, 3)
            let catalog = try XCTUnwrap(
                fields.first {
                    $0.placeholderString == String(localized: "Catalog #")
                }
            )
            for field in dashed + [catalog] {
                let frame = field.convert(field.bounds, to: host)
                XCTAssertTrue(
                    host.isFlipped
                        ? frame.minY >= cover.maxY : frame.maxY <= cover.minY
                )
            }
        }
    }

    /// The pressing fields are part of the card in every state — there is no
    /// fold to open before the year, label and catalog number can be checked.
    func testReleaseFieldsStayInViewWithTheAlbumIdentity() async throws {
        NSApplication.shared.finishLaunching()
        let recorder = MetadataCardActionRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 620)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            let text = editableTextValues(in: host)
            let values = PreviewData.confirmEditValues
            XCTAssertTrue(text.contains(values.albumTitle))
            XCTAssertTrue(text.contains(values.albumYear))
            XCTAssertTrue(text.contains(values.pressing.year))
            for label in values.pressing.labels {
                XCTAssertTrue(text.contains(label.name))
                XCTAssertTrue(text.contains(label.catalogNumber))
            }

        }
    }

    private func assertCardLayout(
        provenance: BridgeMetadataProvenance?
    ) async throws {
        let recorder = MetadataCardActionRecorder()
        let size = NSSize(width: 900, height: 520)
        try await SnapshotTestSupport.withHostedWindow(
            metadataHeader(
                provenance: provenance,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let (identify, search) = try releaseEntryFrames(in: host)
            let menu = try menuFrame(in: host)
            let cover = try coverFrame(in: host)

            // The card's actions have its first row to themselves: none shares a
            // band with the cover, and they read left to right — the entry that
            // asks for a run, the entry that asks for nothing, then the menu of
            // what rewrites the draft.
            XCTAssertFalse(identify.intersects(cover))
            XCTAssertFalse(search.intersects(cover))
            XCTAssertFalse(menu.intersects(cover))
            XCTAssertTrue(
                identify.maxY <= cover.minY || identify.minY >= cover.maxY
            )
            XCTAssertLessThan(identify.maxX, search.minX)
            XCTAssertLessThan(search.maxX, menu.minX)
            try HostedInput.click(at: identify.center, in: host)
            XCTAssertEqual(recorder.identifyCount, 1)
            XCTAssertEqual(recorder.searchCount, 0)
            try HostedInput.click(at: search.center, in: host)
            XCTAssertEqual(recorder.identifyCount, 1)
            XCTAssertEqual(recorder.searchCount, 1)

        }
    }

    private func metadataHeader(
        provenance: BridgeMetadataProvenance?,
        draftIsBlank: Bool,
        recorder: MetadataCardActionRecorder
    ) -> some View {
        let editValues =
            draftIsBlank
            ? MappingFixtures.blankEdit : PreviewData.confirmEditValues
        let candidate = Candidate(
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.thirteenFileTable,
                edit: editValues,
                metadataProvenance: provenance
            )
        )
        return ImportReleaseHeader(
            releaseSummary: ImportReleaseSummary(
                candidate: candidate,
                editValues: editValues
            ),
            actionable: true,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: false,
            editValues: editValues,
            records: [],
            editActions: ReleaseFieldWriter { _, _ in },
            editingCommands: EditingCommitCommands(),
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                identifyAutomatically: { recorder.identifyCount += 1 },
                searchForRelease: { recorder.searchCount += 1 },
                reset: { recorder.resetCount += 1 },
                resetToFileMetadata: { recorder.tagsCount += 1 },
                clearMetadata: { recorder.clearCount += 1 },
                unlink: {}
            ),
            localCoverSelections: [:],
            onEditCover: {},
            onSelectCover: { _ in }
        )
        .frame(width: 900, height: 520)
        .importPreviewEnvironment()
        .environment(Library.stub())
    }

    /// The two buttons the card puts in the key-view loop, in reading order:
    /// the two ways to a release. Everything that rewrites the draft is in the
    /// menu beside them.
    private func releaseEntryFrames(
        in host: NSView
    ) throws -> (identify: NSRect, search: NSRect) {
        let controls = focusFrames(in: host)
            .filter { $0.height >= 20 }
            .sorted { $0.minX < $1.minX }
        XCTAssertEqual(controls.count, 2)
        return (try XCTUnwrap(controls.first), try XCTUnwrap(controls.last))
    }

    /// The menu the two draft commands live behind.
    private func menuFrame(in host: NSView) throws -> NSRect {
        let menu = try sourceMenuButton(in: host)
        return menu.convert(menu.bounds, to: host)
    }

    /// The card's own menu, told apart from the release grid's fact menus by
    /// what it holds: the command that resets the draft.
    private func sourceMenuButton(in host: NSView) throws -> NSPopUpButton {
        let menus = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSPopUpButton }
            .filter { button in
                SnapshotTestSupport.populateMenu(button)
                return (button.menu?.indexOfItem(withTitle: "Reset") ?? -1) >= 0
            }
        XCTAssertEqual(menus.count, 1)
        return try XCTUnwrap(menus.first)
    }

    private func focusFrames(in host: NSView) -> [NSRect] {
        focusViews(in: host)
            .map { $0.convert($0.bounds, to: host) }
    }

    private func editableTextValues(in host: NSView) -> [String] {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { view in
                guard let field = view as? NSTextField, field.isEditable else {
                    return nil
                }
                return field.stringValue
            }
    }

    private func coverFrame(in host: NSView) throws -> NSRect {
        let side = ImportCoverWell.coverSize
        let frames = SnapshotTestSupport.descendants(of: host)
            .filter { $0.bounds.width == side && $0.bounds.height == side }
            .map { $0.convert($0.bounds, to: host) }
        let frame = try XCTUnwrap(frames.first)
        XCTAssertTrue(frames.allSatisfy { $0 == frame })
        return frame
    }

    private func focusViews(in host: NSView) -> [NSView] {
        host.subviews.filter {
            $0.nextKeyView != nil || $0.previousKeyView != nil
        }
    }
}

extension ImportMetadataCardLayoutTests {
    func testResetConfirmationAndCancellation() async throws {
        let recorder = MetadataCardActionRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 620)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let menu = try sourceMenu(in: host)
            let resetIndex = menu.indexOfItem(withTitle: "Reset")
            XCTAssertGreaterThanOrEqual(resetIndex, 0)
            guard resetIndex >= 0 else { return }
            for confirmation in ["Cancel", "Reset"] {
                menu.performActionForItem(at: resetIndex)
                try await SnapshotTestSupport.settle(host)
                XCTAssertEqual(recorder.resetCount, 0)
                let buttons = NSApplication.shared.windows
                    .filter(\.isVisible)
                    .flatMap { window in
                        window.contentView.map {
                            SnapshotTestSupport.descendants(of: $0)
                                .compactMap { $0 as? NSButton }
                        } ?? []
                    }
                let button = try XCTUnwrap(
                    buttons.first { $0.title == confirmation },
                    "Confirmation buttons: \(buttons.map(\.title))"
                )
                HostedInput.press(button)
                try await SnapshotTestSupport.settle(host)
            }
            XCTAssertEqual(recorder.resetCount, 1)
            XCTAssertEqual(recorder.tagsCount, 0)
            XCTAssertEqual(recorder.clearCount, 0)
        }
    }

    func testMenuOffersFullResetAlongsideMetadataCommands() async throws {
        let recorder = MetadataCardActionRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 620)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let menu = try sourceMenu(in: host)
            for title in ["Reset", "Reset to file metadata", "Clear metadata"] {
                XCTAssertNotNil(
                    menu.item(withTitle: title),
                    "Missing \(title); menu contains \(menu.items.map(\.title))"
                )
            }
        }
    }

    private func sourceMenu(in host: NSView) throws -> NSMenu {
        try XCTUnwrap(sourceMenuButton(in: host).menu)
    }

    func testSourceAudioSummaryHasNoDisclosureControl() async throws {
        NSApplication.shared.finishLaunching()
        let sourceAudio = try XCTUnwrap(
            PreviewData.mappingCandidate.files.sourceAudio
        )
        let size = NSSize(width: 240, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            ImportSourceAudioSummaryView(sourceAudio: sourceAudio)
                .frame(width: size.width, height: size.height)
                .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            XCTAssertTrue(focusFrames(in: host).isEmpty)

        }
    }

    func testAlbumIdentityTitleOutsizesItsYear() async throws {
        NSApplication.shared.finishLaunching()
        let recorder = MetadataCardActionRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 620)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            let textFields = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
            let title = try XCTUnwrap(
                textFields.first {
                    $0.stringValue == PreviewData.confirmEditValues.albumTitle
                }
            )
            let year = try XCTUnwrap(
                textFields.first {
                    $0.stringValue == PreviewData.confirmEditValues.albumYear
                }
            )

            XCTAssertGreaterThan(
                try XCTUnwrap(title.font).pointSize,
                try XCTUnwrap(year.font).pointSize
            )

        }
    }

    /// A draft already read from a release keeps both ways back to Find
    /// online: identifying again is how a person disagrees with the match,
    /// and searching by name is how they go looking for a different one.
    func testMatchedReleaseKeepsTheCardActions() async throws {
        let recorder = MetadataCardActionRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            metadataHeader(
                provenance: .externalRelease(
                    record: BridgeMetadataRef(
                        catalog: .musicBrainz,
                        key: "release-mb"
                    )
                ),
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 520)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            XCTAssertEqual(
                focusFrames(in: host).filter { $0.height >= 20 }.count,
                2
            )
            XCTAssertNoThrow(try menuFrame(in: host))
        }
    }

    /// A release in more than one format names what differs, each fact's
    /// values joined the way the locale lists things.
    func testMixedSourceAudioNamesWhatDiffers() {
        let codecs = BridgeSourceAudioSummary.mixed(differences: [
            .codec(codecs: ["FLAC", "MP3"])
        ])
        XCTAssertEqual(
            codecs.text,
            ListFormatter.localizedString(byJoining: ["FLAC", "MP3"])
        )

        let rateAndDepth = BridgeSourceAudioSummary.mixed(differences: [
            .sampleRate(sampleRatesHz: [44_100, 96_000]),
            .bitDepth(bitsPerSample: [16, 24]),
        ])
        XCTAssertEqual(
            rateAndDepth.text,
            [
                ListFormatter.localizedString(byJoining: [
                    "44.1\u{00a0}kHz", "96\u{00a0}kHz",
                ]),
                ListFormatter.localizedString(byJoining: [
                    "16\u{2011}bit", "24\u{2011}bit",
                ]),
            ]
            .joined(separator: coreString("core.audio.list_separator"))
        )
    }

    func testSourceAudioFactsBreakOnlyBetweenComponents() {
        let summary = BridgeSourceAudioSummary.uniform(
            descriptor: BridgeSourceAudioDescriptor(
                layout: .cue,
                format: MappingFixtures.audioFormat
            )
        )

        XCTAssertEqual(
            summary.text,
            "FLAC · 44.1\u{00a0}kHz · 16\u{2011}bit · stereo"
        )
    }
}

@MainActor
private final class MetadataCardActionRecorder {
    var identifyCount = 0
    var searchCount = 0
    var resetCount = 0
    var tagsCount = 0
    var clearCount = 0
}

extension NSRect {
    fileprivate var center: NSPoint {
        NSPoint(x: midX, y: midY)
    }
}

/// Every presentation write the store made, so a test can read what it would
/// have stored with the candidate.
