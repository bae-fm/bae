import AppKit
import BaeKit
import SwiftUI
import Testing
import XCTest

@testable import bae

private struct ExternalMetadataApplication: Equatable {
    let key: String
    let provenance: BridgeMetadataProvenance
}

@MainActor
private final class MetadataSourceRecorder {
    var externalApplications: [ExternalMetadataApplication] = []
    var fileTagApplications: [String] = []
    var clearedKeys: [String] = []
    var identifiedKeys: [String] = []
    var errors: [String] = []

    var importer: Importer {
        Importer(
            applyCandidateExternalMetadata: { [self] key, provenance in
                await MainActor.run {
                    externalApplications.append(
                        ExternalMetadataApplication(
                            key: key,
                            provenance: provenance
                        )
                    )
                }
                return 1
            },
            applyCandidateFileTags: { [self] key in
                await MainActor.run {
                    fileTagApplications.append(key)
                    return UInt64(fileTagApplications.count)
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
            }
        )
    }

    func services(_ store: ImportStore) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: store,
            endEditing: {},
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
    /// Choosing a surface is written to core, not kept in the pane: the
    /// store records the write, and the next detail is what the pane shows.
    @Test("choosing a surface writes it through, and the detail shows it")
    func choosingASurfaceWritesItThrough() async throws {
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            edit: MappingFixtures.blankEdit,
        )
        let writes = PresentationWriteRecorder()
        store.sessionWriter = .recording { writes.record($0) }

        store.presentMetadata(
            .findOnline,
            forKey: MappingFixtures.candidateKey
        )
        await waitUntil {
            !writes.presentations(forKey: MappingFixtures.candidateKey).isEmpty
        }

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                presentation: .findOnline
            )
        )

        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .findOnline
        )
        #expect(
            writes.presentations(forKey: MappingFixtures.candidateKey)
                == [.findOnline]
        )
    }

    /// Resetting to tags is one command: it replaces the draft with what the
    /// candidate's own files say and leaves the pane on the draft it wrote.
    /// There is no surface to review the tags on first.
    @Test("resetting to tags applies the files' tags to the draft")
    func resetToTagsAppliesTheFilesTags() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()

        ImportMappingFlow.resetToTags(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        await waitUntil {
            recorder.fileTagApplications == [MappingFixtures.candidateKey]
        }

        #expect(recorder.fileTagApplications == [MappingFixtures.candidateKey])
        #expect(recorder.errors.isEmpty)
    }

    @Test("applying an online result stores the draft as where the pane is")
    func onlineApplicationStoresTheDraft() async throws {
        let key = MappingFixtures.candidateKey
        let writes = SessionWriteRecorder()
        // The pane is on Find online, as the candidate's stored session says.
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
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
            provenance: MappingFixtures.provenance
        )
        await waitUntil {
            writes.presentations(forKey: key).last == .draft
        }

        #expect(recorder.externalApplications.map(\.key) == [key])
        #expect(store.metadataApplicationSession(forKey: key) == nil)
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

    /// Identifying opens the page the run reports on and asks core for a run.
    /// Core decides nothing about whether the press counts, and neither does
    /// this: every press is a run.
    @Test("identifying opens the page and starts a run every time")
    func identifyingStartsARunEveryTime() async throws {
        let writes = PresentationWriteRecorder()
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
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
        // The presentation write goes to core and comes back; the run request
        // is fire-and-forget and is already recorded.
        await waitUntil {
            writes.presentations(forKey: MappingFixtures.candidateKey).count
                == 2
        }

        #expect(
            recorder.identifiedKeys
                == [MappingFixtures.candidateKey, MappingFixtures.candidateKey]
        )
        #expect(
            writes.presentations(forKey: MappingFixtures.candidateKey)
                == [.findOnline, .findOnline]
        )
    }

    /// Searching for a release opens the same page and asks for nothing: what
    /// it offers is the typed form, and a run is the other entry's to start.
    @Test("searching for a release opens the page and starts no run")
    func searchingForAReleaseStartsNoRun() async throws {
        let writes = PresentationWriteRecorder()
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            edit: MappingFixtures.blankEdit
        )
        store.sessionWriter = .recording { writes.record($0) }
        let recorder = MetadataSourceRecorder()
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.presentMetadata(
            .findOnline,
            for: candidate,
            services: recorder.services(store)
        )
        await waitUntil {
            !writes.presentations(forKey: MappingFixtures.candidateKey).isEmpty
        }

        #expect(recorder.identifiedKeys.isEmpty)
        #expect(
            writes.presentations(forKey: MappingFixtures.candidateKey)
                == [.findOnline]
        )
    }

    @Test("clearing metadata dispatches the candidate command")
    func clearMetadataDispatchesCommand() async {
        let store = MappingFixtures.store(mapping: nil)
        let recorder = MetadataSourceRecorder()

        ImportMappingFlow.clearMetadata(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        await waitUntil { !recorder.clearedKeys.isEmpty }

        #expect(recorder.clearedKeys == [MappingFixtures.candidateKey])
        #expect(recorder.errors.isEmpty)
    }

    private func waitUntil(_ predicate: () -> Bool) async {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        #expect(predicate())
    }
}

/// What the draft card offers as ways to a release.
@MainActor
@Suite("The draft card's release entries")
struct ImportReleaseEntryTests {
    /// Two entries, named for what each one does rather than where it goes:
    /// both open the same page, and only the first asks for a run.
    @Test("the card names both ways to a release")
    func theCardNamesBothWaysToARelease() async throws {
        let lines = try await FindOnlineRendering.text(
            ImportReleaseHeader(
                releaseSummary: ImportReleaseSummary(
                    candidate: PreviewData.mappingCandidate,
                    editValues: PreviewData.confirmEditValues
                ),
                isReading: false,
                coverContent: nil,
                hasCoverOptions: true,
                editValues: PreviewData.confirmEditValues,
                editActions: ReleaseFieldWriter { _, _ in },
                editingCommands: EditingCommitCommands(),
                commit: nil,
                sourceActions: ImportReleaseSourceActions(
                    identifyAutomatically: {},
                    searchForRelease: {},
                    resetToTags: {},
                    clearMetadata: {}
                ),
                localCoverSelections: [:],
                onEditCover: {},
                onSelectCover: { _ in }
            )
            .importPreviewEnvironment()
            .environment(Library.stub())
            .candidateReaderPreviewEnvironment(),
            size: NSSize(width: 900, height: 420)
        )

        for label in [
            String(localized: "Identify automatically"),
            String(localized: "Search for release"),
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
            ImportMappingFlow.resetToTags(key: key, services: services)
            try await waitUntil {
                recorder.fileTagApplications.count == applications
            }
            store.applyCandidateDetail(
                key: key,
                detail: MappingFixtures.detail(
                    mapping: MappingFixtures.fileTagsTable,
                    metadataProvenance: .fileTags,
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
            .fileTags
        )
    }

    private func waitUntil(_ predicate: () -> Bool) async throws {
        for _ in 0..<100 where !predicate() {
            await Task.yield()
        }
        _ = try XCTUnwrap(predicate() ? true : nil)
    }
}

@MainActor
final class ImportMetadataCardLayoutTests: XCTestCase {
    func testSourceActionsLeadTheCardAboveCoverAndFields() async throws {
        NSApplication.shared.finishLaunching()
        let provenances: [BridgeMetadataProvenance?] = [
            nil,
            .fileTags,
        ]
        for provenance in provenances {
            try await assertCardLayout(provenance: provenance)
        }
    }

    func testBlankMetadataOpensEditableFieldsBesideTheCover()
        async throws
    {
        NSApplication.shared.finishLaunching()
        let recorder = MetadataCardActionRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: true,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 900)
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let editableFrames = SnapshotTestSupport.descendants(of: host)
            .compactMap { view -> NSRect? in
                guard let field = view as? NSTextField, field.isEditable else {
                    return nil
                }
                return field.convert(field.bounds, to: host)
            }
        let cover = try coverFrame(in: host)
        XCTAssertFalse(editableFrames.isEmpty)
        XCTAssertTrue(
            editableFrames.allSatisfy { $0.minX >= cover.maxX }
        )
        window.contentView = nil
        window.orderOut(nil)
    }

    /// The pressing fields are part of the card in every state — there is no
    /// fold to open before the year, label and catalog number can be checked.
    func testReleaseFieldsStayInViewWithTheAlbumIdentity() async throws {
        NSApplication.shared.finishLaunching()
        let recorder = MetadataCardActionRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 620)
        )
        await SnapshotTestSupport.settle(host)

        let text = editableTextValues(in: host)
        let values = PreviewData.confirmEditValues
        XCTAssertTrue(text.contains(values.albumTitle))
        XCTAssertTrue(text.contains(values.albumYear))
        XCTAssertTrue(text.contains(values.pressing.year))
        XCTAssertTrue(text.contains(values.pressing.label))
        XCTAssertTrue(text.contains(values.pressing.catalogNumber))

        window.contentView = nil
        window.orderOut(nil)
    }

    private func assertCardLayout(
        provenance: BridgeMetadataProvenance?
    ) async throws {
        let recorder = MetadataCardActionRecorder()
        let size = NSSize(width: 900, height: 520)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            metadataHeader(
                provenance: provenance,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
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
        try click(at: identify.center, in: host, window: window)
        XCTAssertEqual(recorder.identifyCount, 1)
        XCTAssertEqual(recorder.searchCount, 0)
        try click(at: search.center, in: host, window: window)
        XCTAssertEqual(recorder.identifyCount, 1)
        XCTAssertEqual(recorder.searchCount, 1)

        window.contentView = nil
        window.orderOut(nil)
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
            isReading: false,
            coverContent: nil,
            hasCoverOptions: false,
            editValues: editValues,
            editActions: ReleaseFieldWriter { _, _ in },
            editingCommands: EditingCommitCommands(),
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                identifyAutomatically: { recorder.identifyCount += 1 },
                searchForRelease: { recorder.searchCount += 1 },
                resetToTags: { recorder.resetCount += 1 },
                clearMetadata: { recorder.clearCount += 1 }
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
        let menus = SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSPopUpButton }
        XCTAssertEqual(menus.count, 1)
        let menu = try XCTUnwrap(menus.first)
        return menu.convert(menu.bounds, to: host)
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
        let side = ImportReleaseHeader.coverSize
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

    private func click(
        at point: NSPoint,
        in host: NSView,
        window: NSWindow
    ) throws {
        let windowPoint = host.convert(point, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try XCTUnwrap(
                NSEvent.mouseEvent(
                    with: type,
                    location: windowPoint,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )
            )
            NSApplication.shared.sendEvent(event)
        }
    }
}

extension ImportMetadataCardLayoutTests {
    func testSourceAudioSummaryHasNoDisclosureControl() async throws {
        NSApplication.shared.finishLaunching()
        let sourceAudio = try XCTUnwrap(
            PreviewData.mappingCandidate.files.sourceAudio
        )
        let size = NSSize(width: 240, height: 40)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportSourceAudioSummaryView(sourceAudio: sourceAudio)
                .frame(width: size.width, height: size.height)
                .importPreviewEnvironment(),
            size: size
        )
        await SnapshotTestSupport.settle(host)

        XCTAssertTrue(focusFrames(in: host).isEmpty)

        window.contentView = nil
        window.orderOut(nil)
    }

    func testAlbumIdentityTitleOutsizesItsYear() async throws {
        NSApplication.shared.finishLaunching()
        let recorder = MetadataCardActionRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            metadataHeader(
                provenance: nil,
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 620)
        )
        await SnapshotTestSupport.settle(host)

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

        window.contentView = nil
        window.orderOut(nil)
    }

    /// A draft already read from a release keeps both ways back to Find
    /// online: identifying again is how a person disagrees with the match,
    /// and searching by name is how they go looking for a different one.
    func testMatchedReleaseKeepsTheCardActions() async {
        let recorder = MetadataCardActionRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            metadataHeader(
                provenance: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "release-mb",
                    partners: []
                ),
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 520)
        )
        await SnapshotTestSupport.settle(host)

        XCTAssertEqual(
            focusFrames(in: host).filter { $0.height >= 20 }.count,
            2
        )
        XCTAssertNoThrow(try menuFrame(in: host))
        window.contentView = nil
        window.orderOut(nil)
    }

    /// A pick that paired two sources says so: one chip per source it claims,
    /// the release the draft was read from first, each linking to its own
    /// release page.
    func testPairedProvenanceShowsOneChipPerSource() {
        let paired = BridgeMetadataProvenance.externalRelease(
            source: .musicBrainz,
            releaseId: "release-mb",
            partners: [
                BridgeMetadataRef(
                    source: .discogs,
                    releaseId: "release-discogs"
                )
            ]
        )

        XCTAssertEqual(
            paired.releaseRefs.map(\.source),
            [.musicBrainz, .discogs]
        )
        XCTAssertEqual(
            paired.releaseRefs.map(\.releaseId),
            ["release-mb", "release-discogs"]
        )

        let unpaired = BridgeMetadataProvenance.externalRelease(
            source: .discogs,
            releaseId: "release-discogs",
            partners: []
        )
        XCTAssertEqual(unpaired.releaseRefs.count, 1)
        XCTAssertEqual(BridgeMetadataProvenance.fileTags.releaseRefs, [])
    }

    /// Both chips draw, so a paired pick is visibly two sources rather than
    /// one with a longer label.
    func testPairedProvenanceDrawsBothChips() async throws {
        let paired = try await FindOnlineRendering.pixels(
            metadataHeader(
                provenance: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "release-mb",
                    partners: [
                        BridgeMetadataRef(
                            source: .discogs,
                            releaseId: "release-discogs"
                        )
                    ]
                ),
                draftIsBlank: false,
                recorder: MetadataCardActionRecorder()
            ),
            size: NSSize(width: 900, height: 520)
        )
        let unpaired = try await FindOnlineRendering.pixels(
            metadataHeader(
                provenance: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "release-mb",
                    partners: []
                ),
                draftIsBlank: false,
                recorder: MetadataCardActionRecorder()
            ),
            size: NSSize(width: 900, height: 520)
        )

        XCTAssertNotEqual(paired, unpaired)
    }

    func testSeveralSourceAudioProfilesReadAsVarious() {
        let summary = BridgeSourceAudioSummary.mixed(descriptors: [
            BridgeSourceAudioDescriptor(
                layout: .file,
                format: MappingFixtures.audioFormat
            ),
            BridgeSourceAudioDescriptor(
                layout: .cue,
                format: MappingFixtures.audioFormat
            ),
        ])

        XCTAssertEqual(summary.text, "Various")
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
    var clearCount = 0
}

extension NSRect {
    fileprivate var center: NSPoint {
        NSPoint(x: midX, y: midY)
    }
}

/// Every presentation write the store made, so a test can read what it would
/// have stored with the candidate.
private final class PresentationWriteRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var writes: [CandidateSessionWrite] = []

    func record(_ write: CandidateSessionWrite) {
        lock.withLock { writes.append(write) }
    }

    func presentations(forKey key: String) -> [BridgeMetadataPresentation] {
        lock.withLock {
            writes.compactMap { write in
                if case .presentation(let written, let presentation) = write,
                    written == key
                {
                    return presentation
                }
                return nil
            }
        }
    }
}
