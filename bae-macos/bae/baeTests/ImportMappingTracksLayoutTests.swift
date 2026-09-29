import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import mapping Tracks layout")
struct ImportMappingTracksLayoutTests {
    @MainActor
    @Test(
        "title editor fills its column and starts at the leading edge",
        arguments: [
            ReleaseMetadataTrackColumns.minimumTableWidth,
            ReleaseMetadataTrackColumns.idealTableWidth,
        ] as [CGFloat]
    )
    func titleEditorFillsColumn(tableWidth: CGFloat) async throws {
        let columns = ReleaseMetadataTrackColumns.resolved(
            tableWidth: tableWidth
        )
        let size = NSSize(width: tableWidth, height: 40)
        try await SnapshotTestSupport.withHostedWindow(
            ImportMappingTrackRow(
                mapping: pairedMapping,
                columns: columns,
                previewingTarget: nil,
                editingCommands: EditingCommitCommands(),
                evidence: [],
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(Library.stub())
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
            let leading =
                ImportMappingColumns.rowPadding + columns.source
                + ReleaseMetadataTrackColumns.track + 2
                * ImportMappingColumns.spacing
                + FieldChrome.inlineHorizontalPadding
            #expect(abs(frame.minX - leading) < 1)
            #expect(
                abs(
                    frame.width
                        - (columns.title - 2
                            * FieldChrome.inlineHorizontalPadding)
                ) < 1
            )
        }
    }

    @MainActor
    @Test(
        "playable source leads the row with the standard playback target",
        arguments: [
            ReleaseMetadataTrackColumns.minimumTableWidth,
            ReleaseMetadataTrackColumns.idealTableWidth,
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
            ImportMappingTrackRow(
                mapping: pairedMapping,
                columns: columns,
                previewingTarget: nil,
                editingCommands: EditingCommitCommands(),
                evidence: [],
                actions: actions(recording: recorder)
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)

            // The point is inside the leading Source cell and the outer edge of
            // the 24-point audition target. A smaller target or a Source cell
            // placed later in the row both leave this click unanswered.
            try HostedInput.click(
                at: NSPoint(
                    x: ImportMappingColumns.rowPadding + 22,
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
        let tableWidth = ReleaseMetadataTrackColumns.idealTableWidth
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
            startSample: 44_100,
            endSample: 88_200
        )
        let hosted = try await hostedTrack(
            tableWidth: ReleaseMetadataTrackColumns.idealTableWidth,
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
            previewing: nil
        )
        let playing = sourceCellHeight(
            source: pairedMapping.source,
            previewing: previewTarget
        )

        #expect(stopped == playing)
        #expect(playing >= ImportMappingSourceCell.auditionTargetSize)
    }

    @MainActor
    @Test("probed duration renders when metadata names no duration")
    func probedDurationRendersWithoutMetadataDuration() async throws {
        let columns = ReleaseMetadataTrackColumns.resolved(
            tableWidth: ReleaseMetadataTrackColumns.idealTableWidth
        )
        let mapping = BridgeTrackMapping(
            source: pairedMapping.source,
            track: pairedMapping.track,
            position: "1",
            durationMs: 180_000,
            lengthsDisagree: false
        )
        let size = NSSize(
            width: ReleaseMetadataTrackColumns.idealTableWidth,
            height: 40
        )
        try await SnapshotTestSupport.withHostedWindow(
            ImportMappingTrackRow(
                mapping: mapping,
                columns: columns,
                previewingTarget: nil,
                editingCommands: EditingCommitCommands(),
                evidence: [],
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .frame(width: size.width, height: size.height, alignment: .leading)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            #expect(mapping.displayedDuration == "3:00")
        }
    }

    @Test("Length shows source and metadata when they disagree")
    func lengthShowsSourceAndMetadataWhenTheyDisagree() {
        let mapping = BridgeTrackMapping(
            source: pairedMapping.source,
            track: pairedMapping.track,
            position: "1",
            durationMs: 210_000,
            lengthsDisagree: true
        )

        #expect(mapping.displayedDuration == "3:00 → 3:30")
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
            (ReleaseMetadataTrackColumns.idealTableWidth, false),
            (1200, true),
        ] as [(CGFloat, Bool)]
    )
    func sheetCaptionControlsKeepTheirOrder(
        tableWidth: CGFloat,
        associated: Bool
    ) async throws {
        let size = NSSize(width: tableWidth, height: 90)
        try await SnapshotTestSupport.withHostedWindow(
            ImportSheetCaptionRow(
                sheet: sheet(associated: associated),
                evidence: [],
                showsDiscMenu: true,
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            let controls = buttons(in: host)
                .sorted {
                    $0.convert($0.bounds, to: host).minX
                        < $1.convert($1.bounds, to: host).minX
                }
            try #require(controls.count == 2)
            let discFrame = controls[0].convert(controls[0].bounds, to: host)
            let bindingFrame = controls[1].convert(controls[1].bounds, to: host)

            #expect(discFrame.minX <= ImportMappingColumns.rowPadding + 1)
            #expect(discFrame.maxX < bindingFrame.minX)
            #expect(bindingFrame.width >= 24)
            #expect(
                bindingFrame.maxX <= tableWidth
                    - ImportMappingColumns.rowPadding
            )
        }
    }

    /// One sheet is one disc, so the pill would only restate it.
    @MainActor
    @Test("a lone sheet's caption has no disc pill")
    func loneSheetCaptionHasNoDiscPill() async throws {
        let size = NSSize(
            width: ReleaseMetadataTrackColumns.idealTableWidth,
            height: 90
        )
        try await SnapshotTestSupport.withHostedWindow(
            ImportSheetCaptionRow(
                sheet: sheet(associated: true),
                evidence: [],
                showsDiscMenu: false,
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .frame(width: size.width, height: size.height, alignment: .leading),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            #expect(buttons(in: host).count == 1)
        }
    }
}

extension ImportMappingTracksLayoutTests {
    @Test("track-sheet source is partitioned from playable rows")
    func trackSheetSourceIsPartitionedFromPlayableRows() {
        let table = BridgeMappingTable(
            images: [],
            trackSections: [
                BridgeMappingTrackSection(
                    side: .flat,
                    headerKey: nil,
                    content: .sheet(
                        sheet: sheet(associated: true),
                        entries: [
                            sheetEntryMapping(
                                number: 1,
                                title: "Source Title"
                            )
                        ]
                    )
                )
            ],
            files: []
        )

        guard
            case .sheet(let sheet, let entries) = table.trackSections[0].content
        else {
            Issue.record("expected a sheet group")
            return
        }
        #expect(sheet.sheetId == "descriptor.cue")
        #expect(entries.map(\.rowId) == ["entry:descriptor.cue:0"])
    }

    @MainActor
    @Test("sheet-entry Source omits the duplicate track number")
    func sheetEntrySourceOmitsDuplicateTrackNumber() {
        let host = NSHostingView(
            rootView: ImportMappingSourceCell(
                source: sheetEntryMapping(number: 42, title: nil).source,
                previewingTarget: nil,
                evidence: [],
                showsFileSize: true,
                actions: actions(recording: MappingTrackActionRecorder())
            )
            .fixedSize()
        )
        host.layoutSubtreeIfNeeded()

        #expect(host.fittingSize.width < 46)
    }

    fileprivate var audioPath: String { "/tmp/source/track.flac" }
    fileprivate var previewTarget: BridgePreviewTarget {
        BridgePreviewTarget(path: audioPath, startSample: 0, endSample: nil)
    }
    fileprivate var longAudioName: String {
        "A very long source filename that must remain inside its column.flac"
    }

    private var pairedMapping: BridgeTrackMapping {
        BridgeTrackMapping(
            source: .file(
                file: BridgeMappingFile(
                    fileId: "track.flac",
                    name: "track.flac",
                    size: 24_000_000,
                    localPath: audioPath,
                    previewTarget: previewTarget,
                    durationMs: 180_000,
                    audioFormat: MappingFixtures.audioFormat,
                    role: .audio,
                    alternatives: [.audio, .notATrack],
                    roleChoice: .audio
                )
            ),
            track: BridgeRawTrackEdit(
                id: "track-1",
                title: "Track Title",
                artistAssignments: .explicit(
                    assignments: [
                        MappingFixtures.artistCredit("Artist Name")
                    ]
                ),
                side: 1,
                trackNumber: 1,
                file: .standalone(fileId: "track.flac")
            ),
            position: "1",
            durationMs: 180_000,
            lengthsDisagree: false
        )
    }

    private func sheet(associated: Bool) -> BridgeSheetGroup {
        BridgeSheetGroup(
            sheetId: "descriptor.cue",
            name:
                "A long descriptor filename that must remain inside Source.cue",
            size: 2_048,
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
            referenceOptions: [
                BridgeSheetReferenceOptions(
                    fileReference: longAudioName,
                    fileId: associated ? longAudioName : nil,
                    options: [
                        BridgeSheetBindingOption(
                            fileId: longAudioName,
                            offer: .offered
                        )
                    ]
                )
            ],
            assignment: .disc(number: 1),
            discOptions: [1, 2]
        )
    }

    private func sheetEntryMapping(
        number: UInt32,
        title: String?
    ) -> BridgeTrackMapping {
        let entry = BridgeMappingEntry(
            sheetId: "descriptor.cue",
            index: number - 1,
            number: number,
            title: title,
            durationMs: 180_000,
            containerId: longAudioName,
            containerName: longAudioName,
            containerLocalPath: audioPath,
            previewTarget: BridgePreviewTarget(
                path: audioPath,
                startSample: UInt64(number - 1) * 44_100,
                endSample: UInt64(number) * 44_100
            ),
            audioFormat: MappingFixtures.audioFormat
        )
        return BridgeTrackMapping(
            source: .sheetEntry(entry: entry),
            track: BridgeRawTrackEdit(
                id: "sheet-track-\(number)",
                title: "Track Title",
                artistAssignments: .explicit(
                    assignments: [
                        MappingFixtures.artistCredit("Artist Name")
                    ]
                ),
                side: 1,
                trackNumber: Int32(number),
                file: .sheetSlice(
                    fileId: entry.containerId,
                    sheetId: entry.sheetId,
                    index: entry.index
                )
            ),
            position: String(number),
            durationMs: entry.durationMs,
            lengthsDisagree: false
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
            ImportMappingTrackRow(
                mapping: pairedMapping,
                columns: columns,
                previewingTarget: previewingTarget,
                editingCommands: EditingCommitCommands(),
                evidence: [],
                actions: actions(recording: recorder)
            )
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(width: tableWidth, height: size.height, alignment: .leading)
            .environment(Library.stub())
            .environment(UiStore()),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            try HostedInput.click(
                at: NSPoint(
                    x: ImportMappingColumns.rowPadding + 22,
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
        source: BridgeMappingSource,
        previewing: BridgePreviewTarget?
    ) -> CGFloat {
        let host = NSHostingView(
            rootView: ImportMappingSourceCell(
                source: source,
                previewingTarget: previewing,
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

    func actions(
        recording recorder: MappingTrackActionRecorder
    ) -> ImportMappingActions {
        ImportMappingActions(
            setRole: { _, _ in },
            bindSheet: { sheet, reference, audio in
                MainActor.assumeIsolated {
                    recorder.sheetBindings.append(
                        SheetAssignmentChange(
                            sheet: sheet,
                            reference: reference,
                            audio: audio
                        )
                    )
                }
            },
            setSheetDisc: { _, _ in },
            openDocument: { _, _ in },
            openImages: { _, _ in },
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
final class MappingTrackActionRecorder {
    var previewed: [BridgePreviewTarget] = []
    var stops = 0
    var sheetBindings: [SheetAssignmentChange] = []
}

struct SheetAssignmentChange: Equatable {
    let sheet: String
    let reference: String
    let audio: String?
}
