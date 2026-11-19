import BaeKit
import SwiftUI

/// Where one release field's settled value goes: a candidate draft persists
/// each field, a persisted-release session updates its working form.
struct ReleaseFieldWriter {
    let setField: @MainActor (BridgeCandidateEditField, String) async -> Void
    /// Every label row, after one is typed into, added or removed.
    let setLabels: @MainActor ([BridgeRawLabelEdit]) async -> Void
    /// Every barcode row, after one is typed into, added or removed.
    let setBarcodes: @MainActor ([String]) async -> Void
    /// A choice of what the pressing is, from one of the form's pickers.
    let setPressingFact: @MainActor (BridgePressingFactEdit) async -> Void
    let setAlbumArtists: @MainActor ([BridgeCredited]) async -> Void

    init(
        setField:
            @escaping @MainActor (
                BridgeCandidateEditField, String
            ) async -> Void,
        setLabels:
            @escaping @MainActor (
                [BridgeRawLabelEdit]
            ) async -> Void = { _ in },
        setBarcodes:
            @escaping @MainActor (
                [String]
            ) async -> Void = { _ in },
        setPressingFact:
            @escaping @MainActor (
                BridgePressingFactEdit
            ) async -> Void = { _ in },
        setAlbumArtists:
            @escaping @MainActor (
                [BridgeCredited]
            ) async -> Void = { _ in }
    ) {
        self.setField = setField
        self.setLabels = setLabels
        self.setBarcodes = setBarcodes
        self.setPressingFact = setPressingFact
        self.setAlbumArtists = setAlbumArtists
    }

}

/// The release header's sizes, shared with the surfaces that draw it and the
/// covers they pass in.
enum ReleaseMetadataLayout {
    /// The cover beside the album identity.
    static let coverSize: CGFloat = 132
    /// Between the header's blocks.
    static let blockSpacing = ThemeSpace.group
}

/// The shared editable release header: the caller's cover beside the album
/// identity, then the release facts.
struct ReleaseMetadataHeader<
    Cover: View,
    AudioFacts: View,
    AlbumRow: View
>: View {
    let values: BridgeRawReleaseEdit
    /// How the album artists stand to the library, as core's read found
    /// them; `nil` while the album names none.
    let albumArtistStanding: BridgeArtistFieldStanding?
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    /// Opens the live artist search the album artist field reads while open.
    let artistSearch: ArtistSearchOpener
    @ViewBuilder
    let cover: () -> Cover
    /// The audio's codec, rate and depth as the identity column's last line;
    /// empty where nothing has read the files.
    @ViewBuilder
    let audioFacts: () -> AudioFacts
    /// The album the release files under, the identity column's last row;
    /// empty where the surface draws it elsewhere.
    @ViewBuilder
    let albumRow: () -> AlbumRow

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                // Clipped here so a non-square cover never spills over the
                // identity column.
                cover()
                    .frame(
                        width: ReleaseMetadataLayout.coverSize,
                        height: ReleaseMetadataLayout.coverSize
                    )
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                    )
                MacReleaseIdentity(
                    values: values,
                    albumArtistStanding: albumArtistStanding,
                    writer: writer,
                    editingCommands: editingCommands,
                    artistSearch: artistSearch,
                    audioFacts: audioFacts,
                    albumRow: albumRow
                )
            }
            MacReleaseFields(
                values: values,
                writer: writer,
                editingCommands: editingCommands
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

extension BridgeArtistAssignment {
    /// The name this credit prints: a picked artist's chosen credit name or
    /// its own; a credit's printed name or its name.
    var displayName: String {
        switch self {
        case .picked(let artist, let creditName):
            creditName?.name ?? artist.name
        case .credit(let credit): credit.creditedAs ?? credit.name
        }
    }
}

extension BridgeCredited {
    /// A library artist in a new place of a credit list, printed by
    /// `creditName` (one of its other names) or its own name.
    static func picked(
        _ artist: BridgeExistingArtist,
        creditName: BridgeCreditName? = nil
    ) -> Self {
        BridgeCredited(
            artist: .picked(artist: artist, creditName: creditName),
            join: nil
        )
    }

    /// A typed name in a new place of a credit list.
    static func typed(_ name: String) -> Self {
        BridgeCredited(
            artist: .credit(
                credit: BridgeArtistCredit(
                    name: name,
                    sortName: nil,
                    musicbrainzArtistId: nil,
                    discogsArtistId: nil,
                    creditedAs: nil
                )
            ),
            join: nil
        )
    }
}

extension BridgeArtistStanding {
    /// The badge naming how one artist stands to the library.
    var label: String {
        switch self {
        case .library: String(localized: "Library")
        case .new: String(localized: "New")
        case .choose(let choices):
            String(localized: "\(choices.count) in library")
        }
    }
}

extension BridgeArtistsStanding {
    /// The badge naming how a whole field's artists stand to the library.
    var label: String {
        switch self {
        case .library: String(localized: "Library")
        case .new: String(localized: "New")
        case .someNew(let count): String(localized: "\(Int(count)) new")
        case .choose(let choices):
            String(localized: "\(Int(choices)) in library")
        case .someToChoose(let count):
            String(localized: "\(Int(count)) to choose")
        }
    }
}

/// A closed artist field's text: the names as one localized list and the
/// badge core's read gives the whole set.
struct ArtistAssignmentsSummary: Equatable {
    let names: String
    let identityLabel: String?

    /// `nil` when nothing is assigned, so the field shows its placeholder.
    init?(
        assignments: [BridgeCredited],
        standing: BridgeArtistsStanding?
    ) {
        guard !assignments.isEmpty else { return nil }
        names = bridgeCreditLine(credits: assignments).text
        identityLabel = standing?.label
    }
}

struct ArtistAssignmentLabel: View {
    let assignment: BridgeArtistAssignment
    let standing: BridgeArtistStanding?

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(assignment.displayName)
                .cutLine(.prose)
            if let standing {
                StatusChip(verbatim: standing.label)
                    .fixedSize()
            }
        }
    }
}

struct ArtistSearchResultLabel: View {
    let artist: BridgeExistingArtist

    var body: some View {
        Text(artist.name)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// One library artist the search found, with its other printed names under
/// it; a row whose page has not answered yet holds its place.
struct ArtistSearchResultRow: View {
    let result: BridgeArtistSearchResult?
    /// Credit the artist, by its own name or one of its other names.
    let onPick: (BridgeCredited) -> Void
    let onViewArtist: (String) -> Void

    var body: some View {
        if let result {
            VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                Button {
                    onPick(.picked(result.artist))
                } label: {
                    ArtistSearchResultLabel(artist: result.artist)
                }
                .buttonStyle(.accentPlain)
                // The artist's other printed names, under it: picking one
                // credits the artist printed by that name.
                ForEach(result.creditNames, id: \.credit.key) { name in
                    Button {
                        onPick(.picked(result.artist, creditName: name.credit))
                    } label: {
                        Text(verbatim: name.credit.name)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.accentPlain)
                    .padding(.leading, ThemeSpace.group)
                }
                MacButton("View in library", kind: .borderless) {
                    onViewArtist(result.artist.artistId)
                }
                .themeText(.detail)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        else {
            // A name's line, so the list keeps its height while the page
            // loads.
            Text(verbatim: " ")
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
