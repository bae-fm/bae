import AppKit
import BaeKit
import SwiftUI
import Testing
import XCTest

@testable import bae

private struct ExternalMetadataApplication: Equatable {
    let key: String
    let source: BridgeMetadataSource
    let releaseId: String
}

@MainActor
private final class MetadataSourceRecorder {
    var externalApplications: [ExternalMetadataApplication] = []
    var fileTagApplications: [String] = []
    var clearedKeys: [String] = []
    var previewedKeys: [String] = []
    var identifiedKeys: [String] = []
    var errors: [String] = []
    var previewResults = [MappingFixtures.albumSeed]
    var previewFailure: (any Error)?

    var importer: Importer {
        Importer(
            applyCandidateExternalMetadata: { [self] key, source, releaseId in
                await MainActor.run {
                    externalApplications.append(
                        ExternalMetadataApplication(
                            key: key,
                            source: source,
                            releaseId: releaseId
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
            previewFileTags: { [self] key in
                try await MainActor.run {
                    previewedKeys.append(key)
                    if let previewFailure { throw previewFailure }
                    precondition(!previewResults.isEmpty)
                    return previewResults.removeFirst()
                }
            },
            identifyForExplicitLookup: { [self] key in
                identifiedKeys.append(key)
            }
        )
    }

    func services(
        _ store: ImportStore,
        defaultFindOnlineMode: BridgeDefaultFindOnlineMode = .automatic
    ) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            defaultFindOnlineMode: defaultFindOnlineMode,
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
    @Test("an unpopulated draft opens its configured source")
    func unpopulatedDraftOpensConfiguredSource() throws {
        for (source, expected) in [
            (
                BridgeDefaultImportMetadataSource.findOnline,
                CandidateMetadataPresentation.findOnline
            ),
            (.fileTags, .fileTags),
            (.none, .draft),
        ] {
            let detail = MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                initialMetadataSource: source
            )

            #expect(Candidate(detail: detail).metadataPresentation == expected)
        }
    }

    @Test("an applied source opens the editable draft")
    func appliedSourceOpensDraft() {
        for provenance in [
            BridgeMetadataProvenance.fileTags,
            .externalRelease(source: .musicBrainz, releaseId: "release"),
        ] {
            let detail = MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: provenance,
                initialMetadataSource: .findOnline
            )

            #expect(
                Candidate(detail: detail).metadataPresentation == .draft
            )
        }
    }

    @Test("a detail refresh preserves the open source browser")
    func detailRefreshPreservesPresentation() throws {
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            edit: MappingFixtures.blankEdit,
            initialMetadataSource: .none
        )
        store.presentMetadata(
            .fileTags,
            forKey: MappingFixtures.candidateKey
        )

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                initialMetadataSource: .findOnline
            )
        )

        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .fileTags
        )
    }

    @Test("Find online starts only in the configured automatic method")
    func findOnlineHonorsConfiguredMethod() throws {
        for mode in [
            BridgeDefaultFindOnlineMode.searchManually,
            .automatic,
        ] {
            let store = MappingFixtures.store(
                mapping: nil,
                metadataProvenance: nil,
                edit: MappingFixtures.blankEdit
            )
            let recorder = MetadataSourceRecorder()
            let candidate = try #require(
                store.candidate(forKey: MappingFixtures.candidateKey)
            )

            ImportMappingFlow.presentMetadata(
                .findOnline,
                for: candidate,
                services: recorder.services(
                    store,
                    defaultFindOnlineMode: mode
                )
            )

            #expect(
                recorder.identifiedKeys
                    == (mode == .automatic
                        ? [MappingFixtures.candidateKey] : [])
            )
        }
    }

    @Test("File tags are previewed without changing the draft")
    func fileTagsPreviewDoesNotApply() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()
        let before = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        let beforeDetail = try #require(before.detail)

        ImportMappingFlow.presentMetadata(
            .fileTags,
            for: before,
            services: recorder.services(store)
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .fileTagsPreview.edit != nil
        }

        let previewing = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(previewing.metadataPresentation == .fileTags)
        #expect(previewing.metadataProvenance == MappingFixtures.provenance)
        #expect(previewing.detail == beforeDetail)
        #expect(recorder.previewedKeys == [MappingFixtures.candidateKey])
        #expect(recorder.fileTagApplications.isEmpty)
    }

    @Test("applying File tags waits for its authoritative detail")
    func fileTagsApplicationWaitsForDetail() async throws {
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()
        let services = recorder.services(store)
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        ImportMappingFlow.presentMetadata(
            .fileTags,
            for: candidate,
            services: services
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .fileTagsPreview.edit != nil
        }

        ImportMappingFlow.useFileTags(
            key: MappingFixtures.candidateKey,
            services: services
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataApplicationSession?
                .commandRevision == 1
        }
        #expect(recorder.fileTagApplications == [MappingFixtures.candidateKey])
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .fileTags
        )

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: MappingFixtures.fileTagsTable,
                metadataProvenance: .fileTags
            )
        )

        let applied = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )
        #expect(applied.metadataApplicationSession == nil)
        #expect(applied.metadataPresentation == .draft)
        #expect(applied.metadataProvenance == .fileTags)
    }

    @Test("applying an online result waits for the matching release detail")
    func onlineApplicationWaitsForMatchingDetail() async throws {
        let key = MappingFixtures.candidateKey
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            edit: MappingFixtures.blankEdit
        )
        let recorder = MetadataSourceRecorder()
        store.presentMetadata(.findOnline, forKey: key)

        ImportSearchFlow.applyMetadata(
            importer: recorder.importer,
            importStore: store,
            endEditing: {},
            key: key,
            provenance: MappingFixtures.provenance,
            onConfirmed: {
                Task { @MainActor in
                    store.presentMetadata(.draft, forKey: key)
                }
            }
        )
        await waitUntil {
            store.candidate(forKey: key)?
                .metadataApplicationSession?
                .commandRevision == 1
        }
        #expect(recorder.externalApplications.map(\.key) == [key])
        #expect(
            store.candidate(forKey: key)?
                .metadataPresentation == .findOnline
        )

        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "different"
                )
            )
        )
        #expect(
            store.candidate(forKey: key)?
                .metadataApplicationSession != nil
        )

        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(
                mapping: nil,
                metadataProvenance: MappingFixtures.provenance
            )
        )
        await waitUntil {
            store.candidate(forKey: key)?
                .metadataPresentation == .draft
        }
        #expect(
            store.candidate(forKey: key)?
                .metadataApplicationSession == nil
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

    @Test("a failed File tags preview remains retryable")
    func failedFileTagsPreviewRemainsRetryable() async throws {
        let store = MappingFixtures.store(mapping: nil)
        let recorder = MetadataSourceRecorder()
        recorder.previewFailure = StubError.notImplemented
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.presentMetadata(
            .fileTags,
            for: candidate,
            services: recorder.services(store)
        )
        await waitUntil {
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .fileTagsPreview == .failed
        }

        ImportMappingFlow.loadFileTagsPreview(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        await waitUntil { recorder.previewedKeys.count == 2 }
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .fileTags
        )
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
    func testFileTagsCanBeReadAndAppliedAgainWithoutClearingMetadata()
        async throws
    {
        let key = MappingFixtures.candidateKey
        let store = MappingFixtures.store(
            mapping: MappingFixtures.thirteenFileTable
        )
        let recorder = MetadataSourceRecorder()
        let services = recorder.services(store)
        let previews = repeatablePreviews()
        recorder.previewResults = previews

        for (offset, preview) in previews.enumerated() {
            let revision = UInt64(offset + 1)
            let candidate = try XCTUnwrap(store.candidate(forKey: key))
            if offset > 0 {
                XCTAssertEqual(
                    candidate.fileTagsPreview.edit,
                    previews[offset - 1]
                )
            }
            ImportMappingFlow.presentMetadata(
                .fileTags,
                for: candidate,
                services: services
            )
            XCTAssertTrue(
                try XCTUnwrap(store.candidate(forKey: key))
                    .fileTagsPreview.isLoading
            )
            try await waitUntil {
                recorder.previewedKeys.count == Int(revision)
                    && store.candidate(forKey: key)?.fileTagsPreview.edit
                        == preview
            }

            ImportMappingFlow.useFileTags(key: key, services: services)
            try await waitUntil {
                store.candidate(forKey: key)?
                    .metadataApplicationSession?
                    .commandRevision == revision
            }
            store.applyCandidateDetail(
                key: key,
                detail: MappingFixtures.detail(
                    mapping: MappingFixtures.fileTagsTable,
                    metadataProvenance: .fileTags,
                    metadataRevision: revision
                )
            )
            try await waitUntil {
                store.candidate(forKey: key)?.metadataPresentation == .draft
            }
        }

        XCTAssertEqual(recorder.previewedKeys, [key, key])
        XCTAssertEqual(recorder.fileTagApplications, [key, key])
        XCTAssertEqual(
            store.candidate(forKey: key)?.metadataProvenance,
            .fileTags
        )
    }

    private func repeatablePreviews() -> [BridgeReleaseUserEdit] {
        [
            MappingFixtures.albumSeed,
            BridgeReleaseUserEdit(
                albumTitle: "Updated Album Title",
                albumArtistAssignments: MappingFixtures.albumSeed
                    .albumArtistAssignments,
                albumYear: MappingFixtures.albumSeed.albumYear,
                pressing: MappingFixtures.albumSeed.pressing,
                tracks: MappingFixtures.albumSeed.tracks
            ),
        ]
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
        let layout = try focusLayout(in: host)
        let cover = try coverFrame(in: host)

        // The source controls have the card's first row to themselves: neither
        // shares a band with the cover, and they read left to right.
        XCTAssertFalse(layout.findOnline.intersects(cover))
        XCTAssertFalse(layout.fileTags.intersects(cover))
        XCTAssertTrue(
            layout.findOnline.maxY <= cover.minY
                || layout.findOnline.minY >= cover.maxY
        )
        XCTAssertLessThan(layout.findOnline.maxX, layout.fileTags.minX)
        XCTAssertFalse(layout.findOnline.intersects(layout.fileTags))
        try click(at: layout.findOnline.center, in: host, window: window)
        try click(at: layout.fileTags.center, in: host, window: window)
        XCTAssertEqual(recorder.findOnlineCount, 1)
        XCTAssertEqual(recorder.fileTagsCount, 1)

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
                useFileTags: { recorder.fileTagsCount += 1 },
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

    /// The two source buttons, as the key-view loop finds them, left to right.
    private func focusLayout(
        in host: NSView
    ) throws -> MetadataCardFocusLayout {
        let controls = focusFrames(in: host)
            .filter { $0.height >= 20 }
            .sorted {
                if abs($0.midY - $1.midY) < 2 { return $0.minX < $1.minX }
                return $0.midY < $1.midY
            }
        let findOnline = try XCTUnwrap(controls.first)
        let fileTags = try XCTUnwrap(controls.dropFirst().first)
        XCTAssertEqual(controls.count, 2)
        return MetadataCardFocusLayout(
            findOnline: findOnline,
            fileTags: fileTags
        )
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

    func testMatchedReleaseKeepsBothSourceActions() async {
        let recorder = MetadataCardActionRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            metadataHeader(
                provenance: .externalRelease(
                    source: .musicBrainz,
                    releaseId: "release-mb"
                ),
                draftIsBlank: false,
                recorder: recorder
            ),
            size: NSSize(width: 900, height: 520)
        )
        await SnapshotTestSupport.settle(host)

        let sourceControls = focusFrames(in: host).filter { $0.height >= 20 }
        XCTAssertEqual(sourceControls.count, 2)
        window.contentView = nil
        window.orderOut(nil)
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
    var fileTagsCount = 0
    var clearCount = 0
}

extension NSRect {
    fileprivate var center: NSPoint {
        NSPoint(x: midX, y: midY)
    }
}

private struct MetadataCardFocusLayout {
    let findOnline: NSRect
    let fileTags: NSRect
}
