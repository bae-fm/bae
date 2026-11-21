import BaeKit
import SwiftUI

/// One release track's editable metadata columns; the table around it supplies
/// the source and actions.
struct ReleaseMetadataTrackRow: View {
    let track: BridgeRawTrackEdit
    /// The track's own artists, each with its badge, as core's read found
    /// them; `nil` while the track takes its album's artists or names none.
    let artists: BridgeArtistField?
    /// The length the track's audio measures, and the length its source lists
    /// when core found the two more than three seconds apart.
    let length: BridgeDurationClock
    let listedLength: BridgeDurationClock?
    let columns: ReleaseMetadataTrackColumns
    let editingCommands: EditingCommitCommands
    /// How the track's place reads ("A1", "3"): core's, and never edited.
    let position: String
    /// Opens the live artist search the artist field reads while open.
    let artistSearch: ArtistSearchOpener
    let onChange: @MainActor (BridgeRawTrackEdit) async -> Void

    var body: some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            Text(verbatim: position)
                .themeText(.body)
                .monospacedDigit()
                .frame(
                    width: ReleaseMetadataTrackColumns.track,
                    alignment: .leading
                )
            MacInlineField(
                placeholder: localizedCoreString("ui.import.slots.untitled"),
                value: track.title,
                name: localizedCoreString("ui.import.mapping.column.title"),
                fillsWidth: true,
                editingCommands: editingCommands,
                onCommit: { value in
                    var edited = track
                    edited.title = value
                    await onChange(edited)
                },
            )
            .frame(width: columns.title)
            artistField
            TrackLengthText(length: length, listedLength: listedLength)
                .frame(
                    width: ReleaseMetadataTrackColumns.length,
                    alignment: .trailing
                )
        }
    }

    private var artistField: some View {
        MacAlbumArtistField(
            field: artists,
            placeholder: localizedCoreString("ui.import.mapping.column.artist"),
            width: .column,
            inheritsAlbumArtists: inheritsAlbumArtists,
            onUseAlbumArtists: {
                updateArtists(.albumArtists)
            },
            artistSearch: artistSearch,
            onChange: {
                updateArtists(.explicit(assignments: $0))
            },
        )
        .modifier(FieldChrome(focused: false, style: .inline))
        .frame(width: columns.artist)
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

/// Column widths shared by every release metadata table, as the board's grid
/// lays them out: Track and Length keep their widths, and Source, Title and
/// Artist share what is left in the ratio 1.2 : 1 : 1, none under its floor.
struct ReleaseMetadataTrackColumns {
    let source: CGFloat
    let title: CGFloat
    let artist: CGFloat

    static let track: CGFloat = 52
    static let length: CGFloat = 88
    /// The board's column gap, which no space token names.
    static let spacing: CGFloat = 10
    static let rowPadding: CGFloat = 0

    private static let floorSource: CGFloat = 160
    private static let floorTitle: CGFloat = 120
    private static let floorArtist: CGFloat = 100
    private static let shareSource: CGFloat = 1.2
    private static let shareTitle: CGFloat = 1
    private static let shareArtist: CGFloat = 1

    private static let chrome: CGFloat = rowPadding * 2 + spacing * 4
    private static let rigid: CGFloat = track + length

    static let minimumTableWidth: CGFloat =
        floorSource + floorTitle + floorArtist + rigid + chrome

    static func resolved(tableWidth: CGFloat) -> Self {
        let width = max(tableWidth, minimumTableWidth)
        // A column whose share falls under its floor keeps the floor, and
        // the others share the rest, as a grid's minmax() tracks do.
        let columns = [
            (share: shareSource, floor: floorSource),
            (share: shareTitle, floor: floorTitle),
            (share: shareArtist, floor: floorArtist),
        ]
        var widths = [CGFloat?](repeating: nil, count: columns.count)
        var free = width - rigid - chrome
        while true {
            let open = columns.indices.filter { widths[$0] == nil }
            let shares = open.reduce(0) { $0 + columns[$1].share }
            let clamped = open.filter {
                free * columns[$0].share / shares < columns[$0].floor
            }
            if clamped.isEmpty {
                for index in open {
                    widths[index] = free * columns[index].share / shares
                }
                break
            }
            for index in clamped {
                widths[index] = columns[index].floor
                free -= columns[index].floor
            }
        }
        return Self(
            source: widths[0] ?? floorSource,
            title: widths[1] ?? floorTitle,
            artist: widths[2] ?? floorArtist
        )
    }
}

/// Where a track a CUE sheet carves starts in its image ("10:22"), in the
/// source cell under the caption that names the image.
struct TrackStartText: View {
    let start: BridgeDurationClock

    var body: some View {
        Text(DurationClock.label(start))
            .themeText(.body)
            .monospacedDigit()
            .foregroundStyle(Theme.macTextSecondary)
    }
}

/// A track's length: the one its audio measures, then the one its source lists
/// in the warning colour when core found them more than three seconds apart.
struct TrackLengthText: View {
    let length: BridgeDurationClock
    let listedLength: BridgeDurationClock?

    var body: some View {
        HStack(spacing: ThemeSpace.line) {
            Text(DurationClock.label(length))
            if let listedLength {
                Text(DurationClock.label(listedLength))
                    .foregroundStyle(Theme.warning)
                    .help(
                        localizedCoreString("ui.import.mapping.listed_length")
                    )
            }
        }
        .themeText(.body)
        .monospacedDigit()
    }
}
