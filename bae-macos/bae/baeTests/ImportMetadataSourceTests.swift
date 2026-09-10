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
            identifyForExplicitLookup: { [self] key in
                identifiedKeys.append(key)
            }
        )
    }

    func services(
        _ store: ImportStore,
        identifyAutomatically: Bool = true
    ) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            identifyAutomatically: identifyAutomatically,
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
        let findOnline = try findOnlineFrame(in: host)
        let menu = try menuFrame(in: host)
        let cover = try coverFrame(in: host)

        // The card's actions have its first row to themselves: neither shares
        // a band with the cover, and they read left to right — the one that
        // identifies the candidate first, the menu of what rewrites its draft
        // after it.
        XCTAssertFalse(findOnline.intersects(cover))
        XCTAssertFalse(menu.intersects(cover))
        XCTAssertTrue(
            findOnline.maxY <= cover.minY || findOnline.minY >= cover.maxY
        )
        XCTAssertLessThan(findOnline.maxX, menu.minX)
        try click(at: findOnline.center, in: host, window: window)
        XCTAssertEqual(recorder.findOnlineCount, 1)

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
                findOnline: { recorder.findOnlineCount += 1 },
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

    /// The one button the card puts in the key-view loop: identifying the
    /// candidate. Everything that rewrites the draft is in the menu beside it.
    private func findOnlineFrame(in host: NSView) throws -> NSRect {
        let controls = focusFrames(in: host).filter { $0.height >= 20 }
        XCTAssertEqual(controls.count, 1)
        return try XCTUnwrap(controls.first)
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
            1
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
    var findOnlineCount = 0
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
