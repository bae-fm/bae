import AppKit
import BaeKit
import Observation
import SwiftUI
import Testing
import XCTest

@testable import bae

private struct ExternalMetadataApplication: Equatable {
    let key: Data
    let link: BridgePressingLink
}

@MainActor
@Observable
private final class MetadataSourceRecorder {
    var externalApplications: [ExternalMetadataApplication] = []
    var fileTagApplications: [Data] = []
    var clearError: (any Error)?
    var events: [String] = []
    var clearedKeys: [Data] = []
    var identifiedKeys: [Data] = []
    var automaticKeys: [Data] = []
    var errors: [DisplayError] = []

    var importer: Importer {
        Importer(
            applyCandidateExternalMetadata: { [self] key, link, _ in
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
            clearCandidateMetadata: { [self] key in
                try await MainActor.run {
                    events.append("clear")
                    if let clearError { throw clearError }
                    clearedKeys.append(key)
                }
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
            openDocument: { _, _, _ in },
            openImages: { _, _ in },
            onError: { [self] in errors.append($0) }
        )
    }
}

@MainActor
@Suite("Import metadata sources")
struct ImportMetadataSourceTests {}

extension ImportMetadataSourceTests {
    @Test("Clear metadata finishes editing then clears the draft")
    func clearFinishesEditingFirst() async throws {
        let store = MappingFixtures.store(mapping: nil)
        let recorder = MetadataSourceRecorder()
        ImportMappingFlow.clearMetadata(
            key: MappingFixtures.candidateKey,
            services: recorder.services(store)
        )
        try await Wait.until { !recorder.clearedKeys.isEmpty }

        #expect(recorder.clearedKeys == [MappingFixtures.candidateKey])
        #expect(recorder.events == ["end editing", "clear"])
        #expect(recorder.fileTagApplications.isEmpty)
        #expect(recorder.errors.isEmpty)
    }

    @Test("Clear metadata failures reach the existing error presentation")
    func clearReportsFailure() async throws {
        let recorder = MetadataSourceRecorder()
        recorder.clearError = NSError(
            domain: "ClearTest",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: "Source changed"]
        )
        ImportMappingFlow.clearMetadata(
            key: MappingFixtures.candidateKey,
            services: recorder.services(MappingFixtures.store(mapping: nil))
        )
        try await Wait.until { !recorder.errors.isEmpty }

        #expect(
            recorder.errors.map(\.line) == [
                "Could not save that change: Source changed"
            ]
        )
        #expect(recorder.fileTagApplications.isEmpty)
    }
}

extension ImportMetadataSourceTests {
    /// Choosing a surface is the pane's own state: the store moves the pane
    /// at once, writes nothing, and a later read leaves it where it went.
    @Test("moving the pane keeps it where it went across reads")
    func choosingASurfaceStaysInThePane() throws {
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit,
        )

        store.movePane(.findOnline, forKey: MappingFixtures.candidateKey)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .findOnline
        )

        store.applyCandidateDetail(
            key: MappingFixtures.candidateKey,
            detail: MappingFixtures.detail(
                mapping: nil,
                edit: MappingFixtures.blankEdit,
                metadataProvenance: nil,
                releaseLink: nil,
                presentation: .draft
            )
        )

        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .metadataPresentation == .findOnline
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

    @Test("applying an online result puts the pane on the draft once it lands")
    func onlineApplicationStoresTheDraft() async throws {
        let key = MappingFixtures.candidateKey
        // The pane opens on Find online, as the candidate's read says.
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit,
            presentation: .findOnline
        )
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
        // The pick landed, so the pane shows the draft it wrote.
        #expect(
            store.candidate(forKey: key)?.metadataPresentation == .draft
        )
    }

    /// Automatic opens identification's results and asks core, which decides
    /// what it takes: the stored verdict shown as it stood, or a run started.
    /// The store asks for no run itself.
    @Test("Automatic opens the results and asks core every time")
    func automaticAsksCore() async throws {
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit
        )
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
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .panePosition
                == CandidatePanePosition(
                    presentation: .findOnline,
                    findOnlineSection: .automatic
                )
        )
    }

    /// Searching for a release opens the same page and asks for nothing: what
    /// it offers is the typed form, and a run is the other entry's to start.
    @Test("searching for a release opens the page and starts no run")
    func searchingForAReleaseStartsNoRun() throws {
        let store = MappingFixtures.store(
            mapping: nil,
            metadataProvenance: nil,
            releaseLink: nil,
            edit: MappingFixtures.blankEdit
        )
        let recorder = MetadataSourceRecorder()
        let candidate = try #require(
            store.candidate(forKey: MappingFixtures.candidateKey)
        )

        ImportMappingFlow.movePane(
            .search,
            for: candidate,
            services: recorder.services(store)
        )

        #expect(recorder.identifiedKeys.isEmpty)
        #expect(recorder.automaticKeys.isEmpty)
        #expect(
            store.candidate(forKey: MappingFixtures.candidateKey)?
                .panePosition
                == CandidatePanePosition(
                    presentation: .findOnline,
                    findOnlineSection: .search
                )
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
                    metadataProvenance: .fileMetadata
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
        let provenances: [BridgeMetadataProvenance?] = [
            nil,
            .fileMetadata,
        ]
        for provenance in provenances {
            try await assertCardLayout(provenance: provenance)
        }
    }

    /// A blank draft keeps its title and album year beside the cover and the
    /// release's text fields below it: year, one blank label row (its name
    /// and catalog number), barcode, each showing the form's dash and named
    /// for its row.
    func testIdentityFieldsSitBesideTheCoverAndReleaseFieldsUnderIt()
        async throws
    {
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
                XCTAssertEqual(field.accessibilityLabel(), placeholder)
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
            XCTAssertEqual(
                Set(dashed.compactMap { $0.accessibilityLabel() }),
                [
                    String(localized: "Year"),
                    String(localized: "Label"),
                    String(localized: "Catalog number"),
                    String(localized: "Barcode"),
                ]
            )
            for field in dashed {
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
                XCTAssertTrue(text.contains(label.label.name))
                XCTAssertTrue(text.contains(label.label.catalogNumber))
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
                summaryCatalogNumber: draftIsBlank ? nil : "AB 100",
                metadataProvenance: provenance
            )
        )
        return MacReleaseEditor(
            actionRow: .importing(nil),
            commands: commands(recording: recorder),
            card: ReleaseEditorCard(
                values: { editValues },
                pressingOffers: PreviewData.pressingOffers,
                albumArtists: nil,
                writer: ReleaseFieldWriter { _, _ in },
                cover: ReleaseCover(
                    content: nil,
                    onEdit: nil,
                    onDropImage: { _ in }
                ),
                sourceAudio: candidate.files.sourceAudio,
                album: nil,
                albumActions: DraftAlbumActions(
                    pick: { _ in },
                    search: { _ in [] }
                ),
                records: [],
                pressingUnknown: false
            ),
            tracks: ReleaseEditorTracks(
                images: [],
                onOpenImages: { _ in },
                sides: [],
                files: [],
                previewingTarget: nil,
                actions: inertMappingActions().trackTable
            ),
            editingCommands: EditingCommitCommands(),
            editable: true,
            searching: false,
            search: { EmptyView() }
        )
        .frame(width: 900, height: 520)
        .importPreviewEnvironment()
        .environment(Library.stub())
    }

    private func commands(
        recording recorder: MetadataCardActionRecorder
    ) -> ReleaseEditorCommands {
        ReleaseEditorCommands(
            identifyAutomatically: { recorder.identifyCount += 1 },
            search: { recorder.searchCount += 1 },
            metadataCommand: { command in
                self.runCommand(command, recording: recorder)
            },
            unlink: {},
            identifyDisabled: false
        )
    }

    private func runCommand(
        _ command: ImportDraftCommand,
        recording recorder: MetadataCardActionRecorder
    ) {
        command.run(
            resetToFileMetadata: { recorder.tagsCount += 1 },
            clearMetadata: { recorder.clearCount += 1 }
        )
    }

    /// The two buttons the card puts in the key-view loop, in reading order:
    /// the two ways to a release. Everything that rewrites the draft is in the
    /// menu beside them.
    private func releaseEntryFrames(
        in host: NSView
    ) throws -> (identify: NSRect, search: NSRect) {
        let menus = HostedMenus.controls(in: host)
            .map { $0.convert($0.bounds, to: host) }
        let controls = focusFrames(in: host)
            .filter { frame in
                frame.height >= 20
                    && !menus.contains {
                        $0.insetBy(dx: -2, dy: -2).contains(frame)
                    }
            }
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
    /// where it sits: in the action row, level with the identify buttons.
    private func sourceMenuButton(in host: NSView) throws -> NSView {
        let (_, search) = try releaseEntryFrames(in: host)
        let menus = HostedMenus.controls(in: host)
            .filter { button in
                let frame = button.convert(button.bounds, to: host)
                return frame.minY < search.maxY && frame.maxY > search.minY
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
        let side = ReleaseCoverWell.coverSize
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
    /// The metadata menu lists every draft command, and each confirmed
    /// command runs its own action and no other.
    func testEachDraftCommandRunsOnlyItsOwnAction() {
        XCTAssertEqual(
            ImportDraftCommand.allCases.map(\.title),
            ["Reset to file metadata", "Clear metadata"]
        )
        // Each command's runs of reset to tags, and clear.
        let runs: [ImportDraftCommand: [Int]] = [
            .resetToFileMetadata: [1, 0],
            .clearMetadata: [0, 1],
        ]
        for command in ImportDraftCommand.allCases {
            let recorder = MetadataCardActionRecorder()
            runCommand(command, recording: recorder)
            XCTAssertEqual(
                [recorder.tagsCount, recorder.clearCount],
                runs[command],
                "\(command)"
            )
            XCTAssertEqual(recorder.identifyCount, 0)
            XCTAssertEqual(recorder.searchCount, 0)
        }
    }

    func testSourceAudioSummaryHasNoDisclosureControl() async throws {
        let sourceAudio = try XCTUnwrap(
            PreviewData.mappingCandidate.files.sourceAudio
        )
        let size = NSSize(width: 240, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            SourceAudioLine(sourceAudio: sourceAudio)
                .frame(width: size.width, height: size.height)
                .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            XCTAssertTrue(focusFrames(in: host).isEmpty)
        }
    }

    func testAlbumIdentityTitleOutsizesItsYear() async throws {
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
                    record: BridgeLookupRef(
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

            XCTAssertNoThrow(try releaseEntryFrames(in: host))
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
            .joined(separator: localizedCoreString("core.audio.list_separator"))
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
            "FLAC · 44.1\u{00a0}kHz · 16\u{2011}bit · Stereo"
        )
    }
}

@MainActor
@Observable
private final class MetadataCardActionRecorder {
    var identifyCount = 0
    var searchCount = 0
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
