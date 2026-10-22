import BaeKit
import SwiftUI

/// What the album row does: pick a library album for the candidate to join,
/// or leave it to the album its release names, and find albums to pick.
struct ImportAlbumActions {
    /// Pick an album by id, or (`nil`) leave it to the release's album.
    let pick: (String?) -> Void
    let search: @MainActor (String) async throws -> [BridgeAlbumChoice]
    let viewInLibrary: (String) -> Void
}

/// The library album the import files under: a new album, the one the
/// release's records name, or one the person picked — with a picker to
/// choose any library album instead.
struct ImportAlbumRow: View {
    let album: BridgeCandidateAlbum
    let canEdit: Bool
    let actions: ImportAlbumActions

    @State
    private var isPresented = false
    @State
    private var query = ""
    @State
    private var results: [BridgeAlbumChoice] = []
    @State
    private var searchError: DisplayError?

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Text("Album")
                .themeText(.detail)
                .foregroundStyle(.secondary)
            value
            Spacer(minLength: 0)
            if canEdit {
                Button(String(localized: "Choose Album…")) {
                    isPresented = true
                }
                .controlSize(.small)
                .popover(isPresented: $isPresented, arrowEdge: .bottom) {
                    picker
                        .frame(width: 320)
                        .padding(ThemeSpace.group)
                }
            }
        }
    }

    @ViewBuilder
    private var value: some View {
        switch album {
        case .new:
            Text("New album").foregroundStyle(.tertiary)
        case .named(let albumId, let title), .picked(let albumId, let title):
            Button {
                actions.viewInLibrary(albumId)
            } label: {
                Text(verbatim: title).lineLimit(1)
            }
            .buttonStyle(.link)
        }
    }

    private var picker: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            if case .picked = album {
                Button(String(localized: "Use the Release's Album")) {
                    actions.pick(nil)
                    isPresented = false
                }
                .buttonStyle(.link)
            }
            TextField("Search", text: $query)
                .textFieldStyle(.roundedBorder)
            if let searchError {
                ErrorDetailDisclosure(error: searchError)
            }
            ForEach(results, id: \.albumId) { result in
                Button {
                    actions.pick(result.albumId)
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
                .buttonStyle(.plain)
            }
        }
        .task(id: query) { await search() }
    }

    /// The album's credit line, then its year where it states one.
    private func line(of result: BridgeAlbumChoice) -> String {
        guard let year = result.year else { return result.artistName }
        return "\(result.artistName) · \(year)"
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
            try await Task.sleep(for: .milliseconds(200))
            results = try await actions.search(trimmed)
        }
        catch is CancellationError {}
        catch {
            results = []
            searchError = DisplayError(error)
        }
    }
}
