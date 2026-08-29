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
            .environment(Library.stub()),
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
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingTrackRow(
                unit: BridgeMappingUnit(
                    source: pairedUnit.source,
                    becomes: .awaitingPick
                ),
                columns: columns.tracks,
                audioChoices: [],
                previewingPath: nil,
                evidence: [],
                actions: actions(recording: recorder)
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(Library.stub()),
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
        withExtendedLifetime(window) {}
    }

    @MainActor
    @Test(
        "descriptor and association stay inside Source",
        arguments: [
            (ImportMappingColumns.minimumTableWidth, false),
            (ImportMappingColumns.minimumTableWidth, true),
            (ImportMappingColumns.idealTableWidth, false),
            (1200, true),
        ] as [(CGFloat, Bool)]
    )
    func descriptorControlsStayInsideSource(
        tableWidth: CGFloat,
        associated: Bool
    ) async throws {
        let columns = ImportMappingColumns.resolved(tableWidth: tableWidth)
        let size = NSSize(width: tableWidth, height: 40)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportMappingSheetRow(
                sheet: sheet(associated: associated),
                columns: columns.tracks,
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
        let sourceEnd =
            ImportMappingColumns.rowPadding + columns.tracks.source
        let associationFrame = controls[0].convert(controls[0].bounds, to: host)
        let discFrame = controls[1].convert(controls[1].bounds, to: host)
        let mappingStart = sourceEnd + ImportMappingColumns.spacing

        #expect(associationFrame.maxX <= sourceEnd)
        #expect(associationFrame.width >= 24)
        // The borderless Menu's AppKit cell bleeds five points beyond its
        // SwiftUI frame. The frame itself starts at the mapped-track span.
        #expect(abs(discFrame.minX - mappingStart) <= 6)
        withExtendedLifetime(window) {}
    }

}

extension ImportMappingTracksLayoutTests {
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
                    probedDurationMs: 180_000,
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
                sourcePosition: "1",
                sourceDurationMs: 180_000
            )
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
                        size: 460_000_000
                    )
                )
                : .unresolved(requested: [longAudioName]),
            assignment: .disc(number: 1),
            discOptions: [1, 2]
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
            .environment(Library.stub()),
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
                lengthsDiverge: false,
                evidence: [],
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
