import BaeKit
import SwiftUI

/// What the album row does: pick a library album for the release to file
/// under, or leave it to the album its records name, and find albums to pick.
struct DraftAlbumActions {
    /// Record what the person picked: a library album, a new album, or the
    /// release's own album.
    let pick: (BridgeAlbumPick) -> Void
    let search: @MainActor (String) async throws -> [BridgeAlbumChoice]
    let viewInLibrary: (String) -> Void
}

/// The library album a draft files under — an import's or a library
/// release's: a new album, the one the release's records name (or, before a
/// catalog is applied, the one it is in), or one the person picked — with a
/// picker to choose any library album instead.
struct DraftAlbumRow: View {
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
    private enum Choice: Hashable {
        case standing
        case newAlbum
        case releasesAlbum
        case showInLibrary
        case searchLibrary
    }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Text("Album")
                .themeText(.detail)
                .foregroundStyle(.secondary)
            if canEdit {
                albumPopUp
            }
            else {
                Text(verbatim: standingTitle)
                    .lineLimit(1)
                    .foregroundStyle(
                        standingAlbumId == nil ? .tertiary : .primary
                    )
            }
            Spacer(minLength: 0)
        }
    }

    private var standingTitle: String {
        switch album {
        case .new, .pickedNew: String(localized: "New album")
        case .named(_, let line), .picked(_, let line):
            switch line {
            case .titleAlone(let title): title
            case .titleAndArtists(let title, let artistLine):
                FactList.joined([title, artistLine.text])
            }
        }
    }

    private var standingAlbumId: String? {
        switch album {
        case .new, .pickedNew: nil
        case .named(let albumId, _), .picked(let albumId, _): albumId
        }
    }

    private var isPicked: Bool {
        switch album {
        case .picked, .pickedNew: true
        case .new, .named: false
        }
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
                    case .showInLibrary:
                        if let albumId = standingAlbumId {
                            actions.viewInLibrary(albumId)
                        }
                    case .searchLibrary:
                        isPresented = true
                    }
                }
            )
        ) {
            MacPopUpOption(
                verbatim: standingTitle,
                value: Choice.standing
            )
            if standingAlbumId != nil {
                MacPopUpOption("New album", value: Choice.newAlbum)
            }
            if isPicked {
                MacPopUpOption(
                    "Use the release’s album",
                    value: Choice.releasesAlbum
                )
            }
            if standingAlbumId != nil {
                MacPopUpOption("Show in library", value: Choice.showInLibrary)
            }
            MacMenuDivider()
            MacPopUpOption(
                "Choose another album…",
                value: Choice.searchLibrary
            )
        }
        .macPopover(isPresented: $isPresented, arrowEdge: .bottom) {
            picker
                .frame(width: 320)
                .padding(ThemeSpace.group)
        }
    }

    private var picker: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            TextField("Search", text: $query)
                .boxedTextField()
            if let searchError {
                Notice(error: searchError)
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
                            .foregroundStyle(.secondary)
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
