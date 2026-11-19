import BaeKit
import SwiftUI

/// One release track's editable metadata columns; the table around it supplies
/// the source and actions.
struct ReleaseMetadataTrackRow: View {
    let track: BridgeRawTrackEdit
    /// How the track's own artists stand to the library, as core's read found
    /// them; `nil` while the track takes its album's artists or names none.
    let artistStanding: BridgeArtistFieldStanding?
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
                    alignment: .leading
                )
        }
    }

    private var artistField: some View {
        MacAlbumArtistField(
            assignments: explicitArtists,
            standing: artistStanding,
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

    private var explicitArtists: [BridgeCredited] {
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

/// Column widths shared by every release metadata table.
struct ReleaseMetadataTrackColumns {
    let source: CGFloat
    let title: CGFloat
    let artist: CGFloat

    static let track: CGFloat = 52
    static let length: CGFloat = 124
    static let spacing = ThemeSpace.related
    static let rowPadding = ThemeSpace.line

    private static let idealTitle: CGFloat = 220
    private static let floorTitle: CGFloat = 120
    private static let idealArtist: CGFloat = 180
    private static let floorArtist: CGFloat = 100
    private static let idealSource: CGFloat = 260
    private static let floorSource: CGFloat = 160

    private static let chrome: CGFloat = rowPadding * 2 + spacing * 4
    private static let rigid: CGFloat = track + length

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

/// Where a track a CUE sheet carves starts in its image ("10:22"), in the
/// source cell under the caption that names the image.
struct TrackStartText: View {
    let start: BridgeDurationClock

    var body: some View {
        Text(DurationClock.label(start))
            .themeText(.body)
            .monospacedDigit()
            .foregroundStyle(.secondary)
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
