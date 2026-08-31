import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import mapping Tracks layout")
struct ImportMappingTracksLayoutTests {
    @Test("artist fill follows the selected row downward")
    func artistFillFollowsOrderedRows() {
        let ordered = ["track-1", "track-2", "track-3", "track-4"]
        var selection = ArtistFillSelection(sourceTrackId: "track-2")

        selection.extend(to: "track-4", in: ordered)

        #expect(
            selection.trackIds(in: ordered) == [
                "track-2", "track-3", "track-4",
            ]
        )
    }

    @MainActor
    @Test(
        "playable source leads the row with the standard playback target",
        arguments: [
            ImportMappingColumns.minimumTableWidth,
            ImportMappingColumns.idealTableWidth,
            1200,
        ] as [CGFloat]
    )
    func playableSourceLeadsTheRow(tableWidth: CGFloat) async throws {
        let columns = ImportMappingColumns.resolved(tableWidth: tableWidth)
        let recorder = MappingTrackActionRecorder()
        let size = NSSize(width: tableWidth, height: 40)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingTrackRow(
                unit: pairedUnit,
                columns: columns.tracks,
                audioChoices: [],
                previewingPath: nil,
                evidence: [],
                actions: actions(recording: recorder)
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        // The point is inside the leading Source cell and the outer edge of
        // the 24-point audition target. A smaller target or a Source cell
        // placed later in the row both leave this click unanswered.
        try click(
            at: NSPoint(
                x: ImportMappingColumns.rowPadding + 22,
                y: size.height / 2
            ),
            in: window
        )
        await Task.yield()

        #expect(recorder.previewed == [audioPath])
        withExtendedLifetime(window) {}
    }

    @MainActor
    @Test("preview state keeps the playable row geometry stable")
    func previewStateKeepsRowGeometryStable() async throws {
        let tableWidth = ImportMappingColumns.idealTableWidth
        let stopped = try await hostedTrack(
            tableWidth: tableWidth,
            previewingPath: nil
        )
        let playing = try await hostedTrack(
            tableWidth: tableWidth,
            previewingPath: audioPath
        )

        #expect(stopped.height == playing.height)
        #expect(stopped.recorder.previewed == [audioPath])
        #expect(playing.recorder.stops == 1)
    }

    @MainActor
    @Test("playback and unavailable Source states keep one row height")
    func sourceStatesKeepOneRowHeight() {
        let stopped = sourceCellHeight(
            source: pairedUnit.source,
            previewing: nil
        )
        let playing = sourceCellHeight(
            source: pairedUnit.source,
            previewing: audioPath
        )
        let unavailable = sourceCellHeight(source: .missing, previewing: nil)

        #expect(stopped == playing)
        #expect(playing == unavailable)
        #expect(unavailable >= ImportMappingSourceCell.auditionTargetSize)
    }

    @MainActor
    @Test("probed duration renders when metadata names no duration")
    func probedDurationRendersWithoutMetadataDuration() async throws {
        let columns = ImportMappingColumns.resolved(
            tableWidth: ImportMappingColumns.idealTableWidth
        )
        let track = try #require(pairedUnit.track)
        let unit = BridgeMappingUnit(
            source: pairedUnit.source,
            becomes: .track(
                track: track,
                sourcePosition: "1"
            ),
            durationMs: 180_000
        )
        let size = NSSize(
            width: ImportMappingColumns.idealTableWidth,
            height: 40
        )
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingTrackRow(
                unit: unit,
                columns: columns.tracks,
                audioChoices: [],
                previewingPath: nil,
                evidence: [],
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .frame(width: size.width, height: size.height, alignment: .leading)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        #expect(unit.displayedDuration == "3:00")
        withExtendedLifetime(window) {}
    }

    @Test("Length shows source and metadata when they disagree")
    func lengthShowsSourceAndMetadataWhenTheyDisagree() throws {
        let track = try #require(pairedUnit.track)
        let unit = BridgeMappingUnit(
            source: pairedUnit.source,
            becomes: .track(track: track, sourcePosition: "1"),
            durationMs: 210_000
        )

        #expect(unit.displayedDuration == "3:00 → 3:30")
    }

    @MainActor
    @Test(
        "awaiting release keeps Source in the same leading cell",
        arguments: [
            ImportMappingColumns.minimumTableWidth,
            ImportMappingColumns.idealTableWidth,
        ] as [CGFloat]
    )
    func awaitingReleaseKeepsSourceLeading(tableWidth: CGFloat) async throws {
        let columns = ImportMappingColumns.resolved(tableWidth: tableWidth)
        let recorder = MappingTrackActionRecorder()
        let size = NSSize(width: tableWidth, height: 40)
        let unit = BridgeMappingUnit(
            source: pairedUnit.source,
            becomes: .awaitingPick,
            durationMs: 180_000
        )
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingTrackRow(
                unit: unit,
                columns: columns.tracks,
                audioChoices: [],
                previewingPath: nil,
                evidence: [],
                actions: actions(recording: recorder)
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        try click(
            at: NSPoint(
                x: ImportMappingColumns.rowPadding + 22,
                y: size.height / 2
            ),
            in: window
        )
        await Task.yield()

        #expect(recorder.previewed == [audioPath])
        #expect(unit.displayedDuration == "3:00")
        withExtendedLifetime(window) {}
    }

    @MainActor
    @Test(
        "sheet source and disc controls stay separated",
        arguments: [
            (ImportMappingColumns.minimumTableWidth, false),
            (ImportMappingColumns.minimumTableWidth, true),
            (ImportMappingColumns.idealTableWidth, false),
            (1200, true),
        ] as [(CGFloat, Bool)]
    )
    func sheetControlsStaySeparated(
        tableWidth: CGFloat,
        associated: Bool
    ) async throws {
        let size = NSSize(width: tableWidth, height: 40)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingSheetRow(
                sheet: sheet(associated: associated),
                options: [
                    BridgeSheetBindingOption(
                        fileId: longAudioName,
                        offer: .offered
                    )
                ],
                evidence: [],
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let controls = buttons(in: host)
            .sorted {
                $0.convert($0.bounds, to: host).minX
                    < $1.convert($1.bounds, to: host).minX
            }
        try #require(controls.count == 2)
        let associationFrame = controls[0].convert(controls[0].bounds, to: host)
        let discFrame = controls[1].convert(controls[1].bounds, to: host)

        #expect(associationFrame.maxX < discFrame.minX)
        #expect(associationFrame.width >= 24)
        #expect(
            discFrame.maxX >= tableWidth - ImportMappingColumns.rowPadding - 6
        )
        withExtendedLifetime(window) {}
    }

}

extension ImportMappingTracksLayoutTests {
    @Test("track-sheet source is partitioned from playable rows")
    func trackSheetSourceIsPartitionedFromPlayableRows() {
        let table = BridgeMappingTable(
            images: [],
            rows: [
                .sheet(
                    sheet: sheet(associated: true),
                    entries: [sheetEntryUnit(number: 1, title: "Source Title")]
                )
            ],
            reconciliation: .agrees(count: 1)
        )

        #expect(table.sheets.map(\.sheetId) == ["descriptor.cue"])
        #expect(table.trackUnits.map(\.rowId) == ["entry:descriptor.cue:0"])
    }

    @MainActor
    @Test("sheet-entry Source omits the duplicate track number")
    func sheetEntrySourceOmitsDuplicateTrackNumber() {
        let host = NSHostingView(
            rootView: ImportMappingSourceCell(
                source: sheetEntryUnit(number: 42, title: nil).source,
                previewingPath: nil,
                evidence: [],
                showsFileSize: true,
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .fixedSize()
        )
        host.layoutSubtreeIfNeeded()

        #expect(host.fittingSize.width < 46)
    }

    @Test(
        "Files reserve a fixed trailing Size column",
        arguments: [
            ImportMappingColumns.minimumTableWidth,
            ImportMappingColumns.idealTableWidth,
            1200,
        ] as [CGFloat]
    )
    func filesReserveTrailingSizeColumn(tableWidth: CGFloat) {
        let columns = ImportMappingColumns.resolved(tableWidth: tableWidth)
        let innerWidth = tableWidth - ImportMappingColumns.rowPadding * 2

        #expect(
            columns.files.name
                == innerWidth - ImportMappingColumns.spacing
                - columns.files.size
        )
        #expect(columns.files.size == 64)
    }

    fileprivate var audioPath: String { "/tmp/source/track.flac" }
    fileprivate var longAudioName: String {
        "A very long source filename that must remain inside its column.flac"
    }

    private var pairedUnit: BridgeMappingUnit {
        BridgeMappingUnit(
            source: .file(
                file: BridgeMappingFile(
                    fileId: "track.flac",
                    name: "track.flac",
                    size: 24_000_000,
                    localPath: audioPath,
                    durationMs: 180_000,
                    audioFormat: MappingFixtures.audioFormat,
                    role: .audio,
                    alternatives: [.audio, .notATrack],
                    roleChoice: .audio
                )
            ),
            becomes: .track(
                track: BridgeRawTrackEdit(
                    id: "track-1",
                    title: "Track Title",
                    artistAssignments: .explicit(
                        assignments: [MappingFixtures.newArtist("Artist Name")]
                    ),
                    side: 1,
                    trackNumber: 1,
                    file: .standalone(fileId: "track.flac")
                ),
                sourcePosition: "1"
            ),
            durationMs: 180_000
        )
    }

    private func sheet(associated: Bool) -> BridgeSheetGroup {
        BridgeSheetGroup(
            sheetId: "descriptor.cue",
            name:
                "A long descriptor filename that must remain inside Source.cue",
            localPath: "/tmp/source/descriptor.cue",
            bound: associated
                ? .describes(
                    container: BridgeMappingContainer(
                        fileId: longAudioName,
                        name: longAudioName,
                        size: 460_000_000,
                        audioFormat: MappingFixtures.audioFormat
                    )
                )
                : .unresolved(requested: [longAudioName]),
            assignment: .disc(number: 1),
            discOptions: [1, 2]
        )
    }

    private func sheetEntryUnit(
        number: UInt32,
        title: String?
    ) -> BridgeMappingUnit {
        let entry = BridgeMappingEntry(
            sheetId: "descriptor.cue",
            index: number - 1,
            number: number,
            title: title,
            durationMs: 180_000,
            containerId: longAudioName,
            containerName: longAudioName,
            containerLocalPath: audioPath,
            audioFormat: MappingFixtures.audioFormat
        )
        return BridgeMappingUnit(
            source: .sheetEntry(entry: entry),
            becomes: .track(
                track: BridgeRawTrackEdit(
                    id: "sheet-track-\(number)",
                    title: "Track Title",
                    artistAssignments: .explicit(
                        assignments: [MappingFixtures.newArtist("Artist Name")]
                    ),
                    side: 1,
                    trackNumber: Int32(number),
                    file: .sheetSlice(
                        fileId: entry.containerId,
                        sheetId: entry.sheetId,
                        index: entry.index
                    )
                ),
                sourcePosition: String(number)
            ),
            durationMs: entry.durationMs
        )
    }

    @MainActor
    private func hostedTrack(
        tableWidth: CGFloat,
        previewingPath: String?
    ) async throws -> (height: CGFloat, recorder: MappingTrackActionRecorder) {
        let columns = ImportMappingColumns.resolved(tableWidth: tableWidth)
        let size = NSSize(width: tableWidth, height: 40)
        let recorder = MappingTrackActionRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingTrackRow(
                unit: pairedUnit,
                columns: columns.tracks,
                audioChoices: [],
                previewingPath: previewingPath,
                evidence: [],
                actions: actions(recording: recorder)
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        try click(
            at: NSPoint(
                x: ImportMappingColumns.rowPadding + 22,
                y: size.height / 2
            ),
            in: window
        )
        await Task.yield()
        let result = (height: host.fittingSize.height, recorder: recorder)
        withExtendedLifetime(window) {}
        return result
    }

    @MainActor
    private func sourceCellHeight(
        source: BridgeMappingSource,
        previewing: String?
    ) -> CGFloat {
        let host = NSHostingView(
            rootView: ImportMappingSourceCell(
                source: source,
                previewingPath: previewing,
                evidence: [],
                showsFileSize: true,
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .frame(width: 180, alignment: .leading)
        )
        return host.fittingSize.height
    }

    @MainActor
    private func buttons(in host: NSView) -> [NSButton] {
        SnapshotTestSupport.descendants(of: host).compactMap { $0 as? NSButton }
    }

    private func actions(
        recording recorder: MappingTrackActionRecorder
    ) -> ImportMappingActions {
        ImportMappingActions(
            setRole: { _, _ in },
            bindSheet: { _, _ in },
            setSheetDisc: { _, _ in },
            openDocument: { _, _ in },
            openImages: { _, _ in },
            preview: { path in
                MainActor.assumeIsolated {
                    recorder.previewed.append(path)
                }
            },
            stopPreview: {
                MainActor.assumeIsolated { recorder.stops += 1 }
            },
            editTrack: { _ in },
            setTrackArtists: { _, _ in },
            chooseFile: { _, _ in },
            drop: { _ in },
            exclude: { _ in }
        )
    }

    @MainActor
    private func click(at point: NSPoint, in window: NSWindow) throws {
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
                NSEvent.mouseEvent(
                    with: type,
                    location: point,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )
            )
            window.sendEvent(event)
        }
    }
}

@MainActor
private final class MappingTrackActionRecorder {
    var previewed: [String] = []
    var stops = 0
}
