import BaeKit
import SwiftUI

/// What the album row does: pick a library album for the release to file
/// under, or leave it to the album its records name, and find albums to pick.
struct DraftAlbumActions {
    /// Record what the person picked: a library album, a new album, or the
    /// release's own album.
    let pick: (BridgeAlbumPick) -> Void
    let search: @MainActor (String) async throws -> [BridgeAlbumChoice]
}

/// The library album a draft files under — an import's or a library
/// release's: a new album, the one the release's records name (or, before a
/// catalog is applied, the one it is in), or one the person picked — with a
/// picker to choose any library album instead.
struct MacAlbumPicker: View {
    let album: BridgeDraftAlbum
    let canEdit: Bool
    let actions: DraftAlbumActions

    @Environment(\.debounceClock)
    private var clock

    @State
    private var isPresented = false
    @State
    private var query = ""
    @State
    private var results: [BridgeAlbumChoice] = []
    @State
    private var searchError: DisplayError?

    /// What the pop-up chooses between: the album the draft files under now,
    /// the release's own album again after a pick, or a search of
    /// the library's albums.
    enum Choice: Hashable {
        case standing
        case newAlbum
        case releasesAlbum
        case searchLibrary
    }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Text("Album")
                .themeText(.detail)
                .foregroundStyle(Theme.macTextSecondary)
            albumPopUp
                .disabled(!canEdit)
            Spacer(minLength: 0)
        }
    }

    /// The pop-up's options for `album`: the album as it stands, then what
    /// the person can change it to.
    @MacPopUpBuilder<Choice>
    static func albumOptions(_ album: BridgeDraftAlbum) -> [MacPopUpEntry<
        Choice
    >] {
        MacPopUpOption(verbatim: album.standingTitle, value: Choice.standing)
        if album.standingAlbumId != nil {
            MacPopUpOption("New album", value: Choice.newAlbum)
        }
        if album.isPicked {
            MacPopUpOption(
                "Use the release’s album",
                value: Choice.releasesAlbum
            )
        }
        MacMenuItem.divider
        MacPopUpOption("Choose another album…", value: Choice.searchLibrary)
    }

    /// Choosing the search opens the picker and leaves the choice as it was.
    private var albumPopUp: some View {
        MacPopUpButton(
            "Album",
            selection: Binding(
                get: { Choice.standing },
                set: { choice in
                    switch choice {
                    case .standing: break
                    case .newAlbum:
                        actions.pick(.new)
                    case .releasesAlbum:
                        actions.pick(.records)
                    case .searchLibrary:
                        isPresented = true
                    }
                }
            )
        ) {
            Self.albumOptions(album)
        }
        .macPopover(isPresented: $isPresented, arrowEdge: .bottom) {
            picker
                .frame(width: 320)
                .padding(ThemeSpace.group)
        }
    }

    private var picker: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            BoxedField(placeholder: "Search", text: $query)
            if let searchError {
                MacNotice(error: searchError)
            }
            ForEach(results, id: \.albumId) { result in
                Button {
                    actions.pick(.album(albumId: result.albumId))
                    isPresented = false
                    query = ""
                } label: {
                    VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                        Text(verbatim: result.title)
                        Text(verbatim: line(of: result))
                            .themeText(.detail)
                            .foregroundStyle(Theme.macTextSecondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.accentPlain)
            }
        }
        .task(id: query) { await search() }
    }

    /// The album's credit line, then its year where it states one.
    private func line(of result: BridgeAlbumChoice) -> String {
        FactList.joined([result.artistLine.text, result.year.map(String.init)])
    }

    @MainActor
    private func search() async {
        searchError = nil
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            results = []
            return
        }
        do {
            try await clock.sleep(for: .milliseconds(200))
            results = try await actions.search(trimmed)
        }
        catch is CancellationError {}
        catch {
            results = []
            searchError = DisplayError(error)
        }
    }
}

extension BridgeDraftAlbum {
    /// What the draft files under as it stands, as the album row names it.
    fileprivate var standingTitle: String {
        switch self {
        case .new, .pickedNew: String(localized: "New album")
        case .named(_, let line), .picked(_, let line):
            switch line {
            case .titleAlone(let title): title
            case .titleAndArtists(let title, let artistLine):
                FactList.joined([title, artistLine.text])
            }
        }
    }

    /// The library album the draft files under, `nil` for a new one.
    fileprivate var standingAlbumId: String? {
        switch self {
        case .new, .pickedNew: nil
        case .named(let albumId, _), .picked(let albumId, _): albumId
        }
    }

    /// Whether the person picked the album, not the release's records.
    fileprivate var isPicked: Bool {
        switch self {
        case .picked, .pickedNew: true
        case .new, .named: false
        }
    }
}
