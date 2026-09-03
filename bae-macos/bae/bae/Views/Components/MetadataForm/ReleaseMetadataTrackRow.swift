import BaeKit
import SwiftUI

/// One release track's editable metadata columns. Source evidence and context
/// actions are supplied by the surrounding table.
struct ReleaseMetadataTrackRow: View {
    let track: BridgeRawTrackEdit
    let duration: String
    let durationDiverges: Bool
    let columns: ReleaseMetadataTrackColumns
    let editingCommands: EditingCommitCommands
    let onChange: @MainActor (BridgeRawTrackEdit) async -> Void
    var artistFillCoordinateSpace: String?

    var body: some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            TrackSideCell(value: sideBinding)
                .frame(width: ReleaseMetadataTrackColumns.side)
            TrackNumberCell(value: trackNumberBinding)
                .frame(width: ReleaseMetadataTrackColumns.track)
            CommittedTextField(
                placeholder: coreString("ui.import.slots.untitled"),
                value: track.title,
                chrome: .inline,
                editingCommands: editingCommands,
                onCommit: { value in
                    var edited = track
                    edited.title = value
                    await onChange(edited)
                },
            )
            .frame(width: columns.title)
            artistField
            Text(duration)
                .font(.system(size: 12))
                .monospacedDigit()
                .accessibilityLabel(
                    coreString("ui.import.slots.column.length")
                )
                .accessibilityValue(duration)
                .foregroundStyle(
                    durationDiverges
                        ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary)
                )
                .help(
                    durationDiverges ? String(localized: "Lengths differ") : ""
                )
                .frame(
                    width: ReleaseMetadataTrackColumns.length,
                    alignment: .trailing
                )
        }
    }

    private var artistField: some View {
        ArtistAssignmentsField(
            assignments: explicitArtists,
            placeholder: coreString("ui.import.mapping.column.artist"),
            inheritsAlbumArtists: inheritsAlbumArtists,
            onUseAlbumArtists: {
                updateArtists(.albumArtists)
            },
            onChange: {
                updateArtists(.explicit(assignments: $0))
            },
        )
        .modifier(FieldChrome(focused: false, style: .inline))
        .frame(width: columns.artist)
        .background {
            if let artistFillCoordinateSpace {
                GeometryReader { geometry in
                    Color.clear.preference(
                        key: ArtistCellFramePreferenceKey.self,
                        value: [
                            track.id: geometry.frame(
                                in: .named(artistFillCoordinateSpace)
                            )
                        ]
                    )
                }
            }
        }
    }

    private var sideBinding: Binding<Int32> {
        Binding(
            get: { track.side },
            set: { value in
                var edited = track
                edited.side = value
                Task { await onChange(edited) }
            }
        )
    }

    private var trackNumberBinding: Binding<Int32?> {
        Binding(
            get: { track.trackNumber },
            set: { value in
                var edited = track
                edited.trackNumber = value
                Task { await onChange(edited) }
            }
        )
    }

    private var explicitArtists: [BridgeArtistAssignment] {
        switch track.artistAssignments {
        case .albumArtists: []
        case .explicit(let artists): artists
        }
    }

    private var inheritsAlbumArtists: Bool {
        if case .albumArtists = track.artistAssignments { return true }
        return false
    }

    private func updateArtists(_ assignments: BridgeTrackArtistAssignments) {
        var edited = track
        edited.artistAssignments = assignments
        Task { await onChange(edited) }
    }
}

/// Width contract shared by candidate, completed-import, and Library release
/// metadata tables.
struct ReleaseMetadataTrackColumns {
    let source: CGFloat
    let title: CGFloat
    let artist: CGFloat

    static let side: CGFloat = 68
    static let track: CGFloat = 52
    static let length: CGFloat = 88
    static let action: CGFloat = 24
    static let spacing: CGFloat = 10
    static let rowPadding: CGFloat = 2

    private static let idealTitle: CGFloat = 220
    private static let floorTitle: CGFloat = 120
    private static let idealArtist: CGFloat = 180
    private static let floorArtist: CGFloat = 100
    private static let idealSource: CGFloat = 260
    private static let floorSource: CGFloat = 160

    private static let chrome: CGFloat = rowPadding * 2 + spacing * 6
    private static let rigid: CGFloat = side + track + length + action

    static let idealTableWidth: CGFloat =
        idealTitle + idealArtist + idealSource + rigid + chrome
    static let minimumTableWidth: CGFloat =
        floorTitle + floorArtist + floorSource + rigid + chrome

    static func resolved(tableWidth: CGFloat) -> Self {
        let width = max(tableWidth, minimumTableWidth)
        let fraction =
            width < idealTableWidth
            ? (idealTableWidth - width)
                / ((idealTitle - floorTitle) + (idealArtist - floorArtist)
                    + (idealSource - floorSource))
            : 0
        let title = shrunk(idealTitle, to: floorTitle, by: fraction)
        let artist = shrunk(idealArtist, to: floorArtist, by: fraction)
        return Self(
            source: max(
                floorSource,
                width - rigid - chrome - title - artist
            ),
            title: title,
            artist: artist
        )
    }

    private static func shrunk(
        _ ideal: CGFloat,
        to floor: CGFloat,
        by fraction: CGFloat
    ) -> CGFloat {
        max(floor, ideal - (ideal - floor) * fraction)
    }
}

func releaseDurationText(_ milliseconds: UInt64?) -> String {
    guard let milliseconds else { return "\u{2014}" }
    guard let signedMilliseconds = Int64(exactly: milliseconds) else {
        preconditionFailure("Track duration exceeds Int64 milliseconds")
    }
    return releaseDurationText(signedMilliseconds)
}

func releaseDurationText(_ milliseconds: Int64?) -> String {
    guard let milliseconds else { return "\u{2014}" }
    let label = DurationClock.text(milliseconds)
    return label.isEmpty ? "\u{2014}" : label
}

#if DEBUG
    #Preview("Release metadata track row") {
        @Previewable
        @State
        var track =
            PreviewData.editMetadataDraft(
                trackCount: 1
            )
            .tracks[0]
        let columns = ReleaseMetadataTrackColumns.resolved(tableWidth: 900)
        ReleaseMetadataTrackRow(
            track: track,
            duration: "3:42",
            durationDiverges: false,
            columns: columns,
            editingCommands: EditingCommitCommands(),
            onChange: { track = $0 }
        )
        .padding(24)
        .frame(width: 960)
        .background(Theme.background)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(UiStore())
    }
#endif
