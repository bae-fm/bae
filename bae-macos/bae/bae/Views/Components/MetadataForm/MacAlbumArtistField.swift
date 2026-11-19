import BaeKit
import SwiftUI

struct MacAlbumArtistField: View {
    /// How wide the closed field draws: its value's width on a header line, the
    /// full column in a table.
    enum Width {
        case value
        case column
    }

    let assignments: [BridgeCredited]
    /// How the field's artists stand to the library, as core's read found
    /// them; `nil` while the field names none.
    let standing: BridgeArtistFieldStanding?
    let placeholder: String
    var width: Width = .value
    var inheritsAlbumArtists = false
    var onUseAlbumArtists: (() -> Void)?
    /// Opens the live artist search the open field reads while it is open.
    let artistSearch: ArtistSearchOpener
    let onChange: ([BridgeCredited]) -> Void

    @Environment(UiStore.self)
    private var uiStore
    @State
    private var isPresented = false

    var body: some View {
        Button {
            isPresented = true
        } label: {
            HStack(spacing: ThemeSpace.compact) {
                fieldValue
                if width == .column {
                    Spacer(minLength: 0)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .focusable()
        .accessibilityLabel(Text(verbatim: accessibleName))
        .macPopover(isPresented: $isPresented, arrowEdge: .bottom) {
            ArtistAssignmentsEditor(
                assignments: assignments,
                standing: standing,
                onUseAlbumArtists: onUseAlbumArtists,
                artistSearch: artistSearch,
                onChange: onChange,
                onViewArtist: { artistId in
                    isPresented = false
                    uiStore.navigateToArtist(artistId)
                }
            )
            .frame(width: 320)
            .padding(ThemeSpace.group)
        }
    }

    /// What assistive technology calls the field: the words it shows, its
    /// names then their badge.
    private var accessibleName: String {
        if inheritsAlbumArtists { return String(localized: "Album artist") }
        guard
            let summary = ArtistAssignmentsSummary(
                assignments: assignments,
                standing: standing?.field
            )
        else { return placeholder }
        return [summary.names, summary.identityLabel]
            .compactMap { $0 }
            .joined(separator: " ")
    }

    @ViewBuilder
    private var fieldValue: some View {
        if inheritsAlbumArtists {
            Text("Album artist").foregroundStyle(.tertiary)
        }
        else if let summary = ArtistAssignmentsSummary(
            assignments: assignments,
            standing: standing?.field
        ) {
            HStack(spacing: ThemeSpace.inline) {
                Text(summary.names)
                    .cutLine(.prose)
                if let label = summary.identityLabel {
                    StatusChip(verbatim: label)
                        .fixedSize()
                }
            }
        }
        else {
            Text(placeholder).foregroundStyle(.tertiary)
        }
    }
}

/// The open artist field: the artists assigned, the search box, and the
/// library artists the search finds. The search is its own live query, open
/// while the field is.
private struct ArtistAssignmentsEditor: View {
    /// The tallest the found artists' list draws before it scrolls.
    static let resultsHeight: CGFloat = 280

    let assignments: [BridgeCredited]
    let standing: BridgeArtistFieldStanding?
    let onUseAlbumArtists: (() -> Void)?
    let artistSearch: ArtistSearchOpener
    let onChange: ([BridgeCredited]) -> Void
    /// Show a library artist's page, closing the field.
    let onViewArtist: (String) -> Void

    @State
    private var query = ""
    @State
    private var search: ArtistSearchResults?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            if let onUseAlbumArtists {
                MacButton(
                    "Album artist",
                    kind: .borderless,
                    action: onUseAlbumArtists
                )
            }
            ForEach(Array(rows.enumerated()), id: \.offset) {
                index,
                row in
                let (assignment, standing) = row
                VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                    HStack(spacing: ThemeSpace.related) {
                        ArtistAssignmentLabel(
                            assignment: assignment.artist,
                            standing: standing
                        )
                        Spacer(minLength: 0)
                        Button {
                            var next = assignments
                            next.remove(at: index)
                            onChange(next)
                        } label: {
                            Image(systemName: "xmark")
                        }
                        .buttonStyle(.accentBorderless)
                        .accessibilityLabel(Text("Remove"))
                    }
                    if case .choose(let choices) = standing {
                        choicesList(choices, replacing: index)
                    }
                }
            }
            HStack(spacing: ThemeSpace.related) {
                TextField("Search", text: $query)
                    .boxedTextField()
                    .onSubmit(addTypedArtist)
                Button("Add", action: addTypedArtist)
                    .disabled(trimmedQuery.isEmpty)
            }
            if let error = search?.error {
                MacNotice(error: error)
            }
            if let list = search?.list {
                foundArtists(list)
            }
        }
        .onChange(of: query) { search?.search(query) }
        .task {
            let opened = ArtistSearchResults(open: artistSearch)
            search = opened
            await Task.untilCancelled()
            opened.close()
        }
    }

    /// Each assigned artist beside its badge, which core's read returns in
    /// the field's order; no badges while the read has none.
    private var rows: [(BridgeCredited, BridgeArtistStanding?)] {
        guard let standing else { return assignments.map { ($0, nil) } }
        return Array(zip(assignments, standing.artists.map { Optional($0) }))
    }

    /// What the search finds, each row holding the page it sits in.
    private func foundArtists(_ list: ArtistSearchList) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: ThemeSpace.related) {
                ForEach(0..<list.positionCount, id: \.self) { index in
                    ArtistSearchResultRow(
                        result: list.row(at: index),
                        onPick: { picked in
                            onChange(assignments + [picked])
                            query = ""
                        },
                        onViewArtist: onViewArtist
                    )
                    .task(id: RowLoadID(epoch: list.loadEpoch, index: index)) {
                        await list.holdPage(containing: index)
                    }
                }
            }
        }
        .frame(maxHeight: Self.resultsHeight)
    }

    /// The library artists a credit could be; picking one replaces the credit.
    private func choicesList(
        _ choices: [BridgeExistingArtist],
        replacing index: Int
    ) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            Text("Which one?")
                .themeText(.detail)
                .foregroundStyle(.secondary)
            ForEach(choices, id: \.artistId) { choice in
                Button {
                    var next = assignments
                    next[index] = BridgeCredited(
                        artist: .picked(artist: choice, creditName: nil),
                        join: next[index].join
                    )
                    onChange(next)
                } label: {
                    ArtistSearchResultLabel(artist: choice)
                }
                .buttonStyle(.accentPlain)
            }
        }
        .padding(.leading, ThemeSpace.group)
    }

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func addTypedArtist() {
        guard !trimmedQuery.isEmpty else { return }
        onChange(assignments + [.typed(query)])
        query = ""
    }
}
