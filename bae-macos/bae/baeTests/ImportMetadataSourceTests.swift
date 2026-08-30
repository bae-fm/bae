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
        automaticIdentification: Bool = true
    ) -> ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            automaticIdentification: automaticIdentification,
            importStore: store,
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
    @Test("an applied draft names both replacement sources directly")
    func appliedDraftNamesReplacementSources() async {
        let view = ImportReleaseHeader(
            releaseSummary: ImportReleaseSummary(
                candidate: PreviewData.mappingCandidate,
                editValues: PreviewData.confirmEditValues
            ),
            draftIsBlank: false,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: false,
            editValues: PreviewData.confirmEditValues,
            editActions: ReleaseFieldWriter { _, _ in },
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                findOnline: {},
                useFileTags: {},
                clearMetadata: {}
            ),
            localCoverSelections: [:],
            onEditCover: {},
            onSelectCover: { _ in }
        )
        .frame(width: 900, height: 360)
        .importPreviewEnvironment()
        .environment(Library.stub())

        let size = NSSize(width: 900, height: 360)
        let (window, host) = SnapshotTestSupport.hostInWindow(view, size: size)
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let labels = SnapshotTestSupport.descendants(of: host)
            .compactMap { ($0 as? NSButton)?.title }

        #expect(labels.contains(String(localized: "Find online…")))
        #expect(
            labels.contains(
                coreString("ui.import.metadata.file_tags") + "…"
            )
        )
        withExtendedLifetime(window) {}
    }

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

    @Test("Find online identifies only when automatic identification is on")
    func findOnlineHonorsAutomaticIdentification() throws {
        for enabled in [false, true] {
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
                    automaticIdentification: enabled
                )
            )

            #expect(
                recorder.identifiedKeys
                    == (enabled ? [MappingFixtures.candidateKey] : [])
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
        #expect(
            recorder.externalApplications.map(\.key) == [key]
        )
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
    func testAppliedMetadataKeepsDetailsWithTextAndClearApartFromSources()
        async throws
    {
        NSApplication.shared.finishLaunching()
        let provenances: [BridgeMetadataProvenance?] = [
            nil,
            .fileTags,
            .externalRelease(source: .musicBrainz, releaseId: "release-mb"),
            .externalRelease(source: .discogs, releaseId: "release-discogs"),
        ]
        for provenance in provenances {
            try await assertCardLayout(
                provenance: provenance,
                draftIsBlank: false
            )
        }
    }

    func testBlankMetadataKeepsDetailsBesideTheCoverWithoutClear()
        async throws
    {
        NSApplication.shared.finishLaunching()
        try await assertCardLayout(provenance: nil, draftIsBlank: true)
    }

    func testDetailsDisclosureRendersTheProductionReleaseFields() async throws {
        NSApplication.shared.finishLaunching()
        let size = NSSize(width: 700, height: 900)
        let collapsed = await detailsControlCount(
            expanded: false,
            size: size
        )
        let expanded = await detailsControlCount(
            expanded: true,
            size: size
        )

        XCTAssertGreaterThan(expanded, collapsed)
    }

    private func detailsControlCount(
        expanded: Bool,
        size: NSSize
    ) async -> Int {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportReleaseDetails(
                values: PreviewData.confirmEditValues,
                writer: ReleaseFieldWriter { _, _ in },
                expanded: .constant(expanded)
            )
            .frame(width: size.width)
            .padding(14)
            .environment(Library.stub()),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let count = focusViews(in: host).count
        window.contentView = nil
        window.orderOut(nil)
        return count
    }

    private func assertCardLayout(
        provenance: BridgeMetadataProvenance?,
        draftIsBlank: Bool
    ) async throws {
        let recorder = MetadataCardActionRecorder()
        let size = NSSize(width: 900, height: 420)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            metadataHeader(
                provenance: provenance,
                draftIsBlank: draftIsBlank,
                recorder: recorder
            ),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        let layout = try focusLayout(in: host, draftIsBlank: draftIsBlank)
        let cover = try coverFrame(in: host)

        XCTAssertGreaterThanOrEqual(layout.details.minX, cover.maxX)
        XCTAssertLessThan(layout.details.minX, layout.findOnline.minX)
        XCTAssertGreaterThanOrEqual(
            layout.details.maxX,
            layout.fileTags.maxX
        )
        XCTAssertLessThan(
            abs(
                layout.findOnline.midY
                    - layout.fileTags.midY
            ),
            2
        )
        try click(at: layout.findOnline.center, in: host, window: window)
        try click(at: layout.fileTags.center, in: host, window: window)
        XCTAssertEqual(recorder.findOnlineCount, 1)
        XCTAssertEqual(recorder.fileTagsCount, 1)

        try await assertClearBehavior(
            layout: layout,
            recorder: recorder,
            host: host,
            window: window
        )

        window.contentView = nil
        window.orderOut(nil)
    }

    private func assertClearBehavior(
        layout: MetadataCardFocusLayout,
        recorder: MetadataCardActionRecorder,
        host: NSView,
        window: NSWindow
    ) async throws {
        guard let clear = layout.clear else { return }
        XCTAssertGreaterThan(abs(clear.midY - layout.findOnline.midY), 8)
        try click(at: clear.center, in: host, window: window)
        await Task.yield()
        XCTAssertEqual(recorder.clearCount, 0)
        let confirmation = try XCTUnwrap(
            NSApplication.shared.windows.first { $0 !== window && $0.isVisible }
        )
        let descendants = try XCTUnwrap(confirmation.contentView).subviews
            .flatMap { [$0] + SnapshotTestSupport.descendants(of: $0) }
        let buttons = descendants.compactMap { $0 as? NSButton }
        let labels = descendants.compactMap { $0 as? NSTextField }
            .map(\.stringValue)
        XCTAssertTrue(labels.contains("Clear metadata?"))
        XCTAssertTrue(
            labels.contains(
                "The candidate files and mapping choices will remain unchanged."
            )
        )
        let confirm = try XCTUnwrap(
            buttons.first { $0.title == "Clear metadata" }
        )
        XCTAssertNotNil(buttons.first { $0.title == "Cancel" })
        confirm.performClick(nil)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(recorder.clearCount, 1)
        window.makeKeyAndOrderFront(nil)
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
            draftIsBlank: draftIsBlank,
            isReading: false,
            coverContent: nil,
            hasCoverOptions: false,
            editValues: editValues,
            editActions: ReleaseFieldWriter { _, _ in },
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
        .frame(width: 900, height: 420)
        .importPreviewEnvironment()
        .environment(Library.stub())
    }

    private func focusLayout(
        in host: NSView,
        draftIsBlank: Bool
    ) throws -> MetadataCardFocusLayout {
        let frames = focusFrames(in: host)
        let details = try XCTUnwrap(
            frames.filter { $0.height < 20 }.max { $0.width < $1.width }
        )
        let remaining =
            frames
            .filter { $0.height >= 20 }
            .sorted {
                if abs($0.midY - $1.midY) < 2 { return $0.minX < $1.minX }
                return $0.midY < $1.midY
            }
        let findOnline = try XCTUnwrap(remaining.first)
        let fileTags = try XCTUnwrap(remaining.dropFirst().first)
        let clear = remaining.dropFirst(2).first
        XCTAssertEqual(remaining.count, draftIsBlank ? 2 : 3)
        return MetadataCardFocusLayout(
            details: details,
            findOnline: findOnline,
            fileTags: fileTags,
            clear: clear
        )
    }

    private func focusFrames(in host: NSView) -> [NSRect] {
        focusViews(in: host)
            .map { $0.convert($0.bounds, to: host) }
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
    let details: NSRect
    let findOnline: NSRect
    let fileTags: NSRect
    let clear: NSRect?
}
