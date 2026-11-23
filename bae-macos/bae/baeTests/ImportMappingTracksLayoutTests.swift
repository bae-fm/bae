import AppKit
import BaeKit
import Observation
import SwiftUI
import Testing

@testable import bae

/// A table width with room for every column beyond its floor.
private let wideTableWidth: CGFloat = 840

/// The board centers the 10-point audition glyph at the leading edge.
private let auditionClickX: CGFloat = 5

@Suite("Import mapping Tracks layout")
struct ImportMappingTracksLayoutTests {
    @MainActor
    @Test(
        "title editor fills its column and starts at the leading edge",
        arguments: [
            ReleaseMetadataTrackColumns.minimumTableWidth,
            wideTableWidth,
        ] as [CGFloat]
    )
    func titleEditorFillsColumn(tableWidth: CGFloat) async throws {
        let columns = ReleaseMetadataTrackColumns.resolved(
            tableWidth: tableWidth
        )
        let size = NSSize(width: tableWidth, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            MacImportTrackRow(
                row: pairedMapping,
                columns: columns,
                previewingTarget: nil,
                editingCommands: EditingCommitCommands(),
                actions: actions(recording: MappingTrackActionRecorder())
                    .trackTable,
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) }
            )
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(UiStore()),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let field = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first { $0.stringValue == "Track Title" }
            )
            let frame = field.convert(
                field.alignmentRect(forFrame: field.bounds),
                to: host
            )
            let cell = try #require(field.cell as? any InsetCell)
            let leading =
                columns.source
                + ReleaseMetadataTrackColumns.track + 2
                * ImportMappingColumns.spacing
            #expect(abs(frame.minX + cell.insets.width - leading) < 1)
            #expect(abs(frame.maxX - (leading + columns.title)) < 1)
        }
    }

    @MainActor
    @Test(
        "the Play button works at the leading edge of the source",
        arguments: [
            ReleaseMetadataTrackColumns.minimumTableWidth,
            wideTableWidth,
            1200,
        ] as [CGFloat]
    )
    func playableSourceLeadsTheRow(tableWidth: CGFloat) async throws {
        let columns = ReleaseMetadataTrackColumns.resolved(
            tableWidth: tableWidth
        )
        let recorder = MappingTrackActionRecorder()
        let size = NSSize(width: tableWidth, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            MacImportTrackRow(
                row: pairedMapping,
                columns: columns,
                previewingTarget: nil,
                editingCommands: EditingCommitCommands(),
                actions: actions(recording: recorder).trackTable,
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) }
            )
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(UiStore()),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)

            try HostedInput.click(
                at: NSPoint(
                    x: auditionClickX,
                    y: size.height / 2
                ),
                in: window
            )
            try await Wait.until { !recorder.previewed.isEmpty }

            #expect(recorder.previewed == [previewTarget])
        }
    }

    @MainActor
    @Test("preview state keeps the playable row geometry stable")
    func previewStateKeepsRowGeometryStable() async throws {
        let tableWidth = wideTableWidth
        let stopped = try await hostedTrack(
            tableWidth: tableWidth,
            previewingTarget: nil
        )
        let playing = try await hostedTrack(
            tableWidth: tableWidth,
            previewingTarget: previewTarget
        )

        #expect(stopped.height == playing.height)
        #expect(stopped.recorder.previewed == [previewTarget])
        #expect(playing.recorder.stops == 1)
    }

    @MainActor
    @Test("another CUE window in the same file does not mark this row playing")
    func cueWindowsHaveDistinctPreviewIdentity() async throws {
        let otherWindow = BridgePreviewTarget(
            path: audioPath,
            name: "track.flac",
            startSample: 44_100,
            endSample: 88_200
        )
        let hosted = try await hostedTrack(
            tableWidth: wideTableWidth,
            previewingTarget: otherWindow
        )

        #expect(hosted.recorder.previewed == [previewTarget])
        #expect(hosted.recorder.stops == 0)
    }

    @MainActor
    @Test("playback states keep one Source row height")
    func sourceStatesKeepOneRowHeight() {
        let stopped = sourceCellHeight(
            source: pairedMapping.source,
            isPreviewing: false
        )
        let playing = sourceCellHeight(
            source: pairedMapping.source,
            isPreviewing: true
        )

        #expect(stopped == playing)
    }

    @MainActor
    @Test("a row with no listed length shows its measured length alone")
    func rowWithNoListedLengthShowsItsMeasuredLength() async throws {
        let columns = ReleaseMetadataTrackColumns.resolved(
            tableWidth: wideTableWidth
        )
        var mapping = pairedMapping
        mapping.artists = nil
        let size = NSSize(
            width: wideTableWidth,
            height: 40
        )
        try await SnapshotTestSupport.withHostedWindow(
            MacImportTrackRow(
                row: mapping,
                columns: columns,
                previewingTarget: nil,
                editingCommands: EditingCommitCommands(),
                actions: actions(recording: MappingTrackActionRecorder())
                    .trackTable,
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) }
            )
            .frame(width: size.width, height: size.height, alignment: .leading)
            .environment(UiStore()),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            #expect(DurationClock.label(mapping.length) == "3:00")
            #expect(mapping.listedLength == nil)
        }
    }

    @Test("A row shows its measured length and, apart, the listed one")
    func rowShowsItsMeasuredLengthAndTheListedOne() throws {
        var mapping = pairedMapping
        mapping.listedLength = BridgeDurationClock.length(ms: 210_000)

        #expect(DurationClock.label(mapping.length) == "3:00")
        #expect(
            DurationClock.label(try #require(mapping.listedLength)) == "3:30"
        )
    }
}

/// The sheet caption over a group of carved rows.
extension ImportMappingTracksLayoutTests {
    /// The caption reads left to right: the disc pill leads, the binding menu
    /// follows the sheet's name, and a long name squeezes neither control out
    /// of the line.
    @MainActor
    @Test(
        "sheet caption keeps the disc pill leading and the binding after it",
        arguments: [
            (ReleaseMetadataTrackColumns.minimumTableWidth, false),
            (ReleaseMetadataTrackColumns.minimumTableWidth, true),
            (wideTableWidth, false),
            (1200, true),
        ] as [(CGFloat, Bool)]
    )
    func sheetCaptionControlsKeepTheirOrder(
        tableWidth: CGFloat,
        associated: Bool
    ) async throws {
        let size = NSSize(width: tableWidth, height: 90)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseSheetCaption(
                sheet: sheet(associated: associated),
                actions: actions(recording: MappingTrackActionRecorder())
                    .sheetCaption
            )
            .frame(width: tableWidth, height: size.height, alignment: .leading),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            let controls = HostedMenus.controls(in: host)
                .sorted {
                    $0.convert($0.bounds, to: host).minX
                        < $1.convert($1.bounds, to: host).minX
                }
            try #require(controls.count == 2)
            let discFrame = controls[0].convert(controls[0].bounds, to: host)
            let bindingFrame = controls[1].convert(controls[1].bounds, to: host)

            #expect(discFrame.minX <= 1)
            #expect(discFrame.maxX < bindingFrame.minX)
            #expect(bindingFrame.width >= 24)
            #expect(bindingFrame.maxX <= tableWidth)
        }
    }
}

extension ImportMappingTracksLayoutTests {
    @Test("track-sheet source is partitioned from playable rows")
    func trackSheetSourceIsPartitionedFromPlayableRows() {
        let table = BridgeMappingTable(
            images: [],
            trackSides: [
                BridgeTrackTableSide(
                    side: .flat,
                    runs: [
                        .sheet(
                            sheet: sheet(associated: true),
                            rows: [sheetEntryMapping(number: 1)]
                        )
                    ]
                )
            ],
            files: [],
            untitledTrackCount: 0
        )

        guard
            case .sheet(let sheet, let rows) = table.trackSides?[0].runs[0]
        else {
            Issue.record("expected a sheet group")
            return
        }
        #expect(sheet.sheetId == "descriptor.cue")
        #expect(rows.map(\.track.id) == ["sheet-track-1"])
    }

    @MainActor
    @Test("sheet-entry Source shows its start, not its track number")
    func sheetEntrySourceShowsItsStartNotItsTrackNumber() {
        func width(number: UInt32) -> CGFloat {
            let host = NSHostingView(
                rootView: TrackSourceCell(
                    source: sheetEntryMapping(number: number).source,
                    audition: .libraryTrack,
                    trackId: "sheet-track-\(number)",
                    isPreviewing: false,
                    actions: actions(recording: MappingTrackActionRecorder())
                        .audition
                )
                .fixedSize()
            )
            host.layoutSubtreeIfNeeded()
            return host.fittingSize.width
        }

        // Entry 1 starts at 0:00 and entry 42 at 0:41: the same width in
        // monospaced digits, which a drawn track number would change.
        #expect(width(number: 42) == width(number: 1))
    }

    fileprivate var audioPath: Data { Data("/tmp/source/track.flac".utf8) }
    fileprivate var previewTarget: BridgePreviewTarget {
        BridgePreviewTarget(
            path: audioPath,
            name: "track.flac",
            startSample: 0,
            endSample: nil
        )
    }
    fileprivate var longAudioName: String {
        "A very long source filename that must remain inside its column.flac"
    }

    /// A track's own artist as the pane's read answers the field: a credit
    /// the library has no artist for.
    private var trackArtists: BridgeArtistField {
        BridgeArtistField(
            standing: .new,
            credits: [
                BridgeStandingCredit(
                    credited: MappingFixtures.artistCredit("Artist Name"),
                    standing: .new
                )
            ]
        )
    }

    private var pairedMapping: BridgeTrackTableRow {
        BridgeTrackTableRow(
            track: BridgeRawTrackEdit(
                id: "track-1",
                title: "Track Title",
                artistAssignments: .explicit(
                    assignments: trackArtists.credits.map(\.credited)
                ),
                file: .standalone(fileId: "track.flac")
            ),
            source: .file(name: "track.flac"),
            audition: .preview(target: previewTarget),
            position: "1",
            length: BridgeDurationClock.length(ms: 180_000),
            listedLength: nil,
            artists: trackArtists,
            showsAlbumArtist: false
        )
    }

    private func sheet(associated: Bool) -> BridgeSheetGroup {
        let reference = BridgeSheetReferenceOptions(
            fileReference: longAudioName,
            bound: associated
                ? BridgeMappingContainer(
                    fileId: longAudioName,
                    name: longAudioName
                ) : nil,
            options: [
                BridgeSheetBindingOption(
                    fileId: longAudioName,
                    name: longAudioName,
                    offer: .offered
                )
            ]
        )
        return BridgeSheetGroup(
            sheetId: "descriptor.cue",
            name:
                "A long descriptor filename that must remain inside Source.cue",
            bound: associated
                ? .describes(
                    container: BridgeMappingContainer(
                        fileId: longAudioName,
                        name: longAudioName
                    )
                )
                : .unresolved(requested: [longAudioName]),
            boundAudio: reference.bound == nil
                ? .none : .one(reference: reference),
            unboundReferences: reference.bound == nil ? [reference] : nil,
            assigned: .disc(1),
            sideChoices: BridgeSheetSideChoice.discs(2)
        )
    }

    /// The sheet's entry `number`, which starts `number - 1` seconds into
    /// its audio.
    private func sheetEntryMapping(number: UInt32) -> BridgeTrackTableRow {
        BridgeTrackTableRow(
            track: BridgeRawTrackEdit(
                id: "sheet-track-\(number)",
                title: "Track Title",
                artistAssignments: .explicit(
                    assignments: trackArtists.credits.map(\.credited)
                ),
                file: .sheetSlice(
                    fileId: longAudioName,
                    sheetId: "descriptor.cue",
                    index: number - 1
                )
            ),
            source: .sheetSlice(
                start: BridgeDurationClock.length(
                    ms: UInt64(number - 1) * 1_000
                )
            ),
            audition: .preview(
                target: BridgePreviewTarget(
                    path: audioPath,
                    name: "track.flac",
                    startSample: UInt64(number - 1) * 44_100,
                    endSample: UInt64(number) * 44_100
                )
            ),
            position: String(number),
            length: BridgeDurationClock.length(ms: 180_000),
            listedLength: nil,
            artists: trackArtists,
            showsAlbumArtist: false
        )
    }

    @MainActor
    private func hostedTrack(
        tableWidth: CGFloat,
        previewingTarget: BridgePreviewTarget?
    ) async throws -> (height: CGFloat, recorder: MappingTrackActionRecorder) {
        let columns = ReleaseMetadataTrackColumns.resolved(
            tableWidth: tableWidth
        )
        let size = NSSize(width: tableWidth, height: 40)
        let recorder = MappingTrackActionRecorder()
        return try await SnapshotTestSupport.withHostedWindow(
            MacImportTrackRow(
                row: pairedMapping,
                columns: columns,
                previewingTarget: previewingTarget,
                editingCommands: EditingCommitCommands(),
                actions: actions(recording: recorder).trackTable,
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) }
            )
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(UiStore()),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            try HostedInput.click(
                at: NSPoint(
                    x: auditionClickX,
                    y: size.height / 2
                ),
                in: window
            )
            try await Wait.until {
                !recorder.previewed.isEmpty || recorder.stops > 0
            }
            let result = (height: host.fittingSize.height, recorder: recorder)
            return result
        }
    }

    @MainActor
    private func sourceCellHeight(
        source: BridgeTrackSource,
        isPreviewing: Bool
    ) -> CGFloat {
        let host = NSHostingView(
            rootView: TrackSourceCell(
                source: source,
                audition: .preview(target: previewTarget),
                trackId: "track",
                isPreviewing: isPreviewing,
                actions: actions(recording: MappingTrackActionRecorder())
                    .audition
            )
            .frame(width: 180, alignment: .leading)
        )
        return host.fittingSize.height
    }

    func actions(
        recording recorder: MappingTrackActionRecorder
    ) -> ImportMappingActions {
        ImportMappingActions(
            bindSheet: { _, _, _ in },
            setSheetPlace: { _, _ in },
            openDocument: { _, _ in },
            openImages: { _ in },
            preview: { target in
                MainActor.assumeIsolated {
                    recorder.previewed.append(target)
                }
            },
            stopPreview: {
                MainActor.assumeIsolated { recorder.stops += 1 }
            },
            editTrack: { _ in },
        )
    }
}

@MainActor
@Observable
final class MappingTrackActionRecorder {
    var previewed: [BridgePreviewTarget] = []
    var stops = 0
}
