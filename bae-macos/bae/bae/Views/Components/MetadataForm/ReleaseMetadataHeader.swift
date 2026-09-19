import BaeKit
import SwiftUI

/// Where one release field's settled value goes. Candidate drafts persist each
/// field independently; persisted-release sessions update their working form.
struct ReleaseFieldWriter {
    let setField: @MainActor (BridgeCandidateEditField, String) async -> Void
    let setAlbumArtists: @MainActor ([BridgeArtistAssignment]) async -> Void

    init(
        setField:
            @escaping @MainActor (
                BridgeCandidateEditField, String
            ) async -> Void,
        setAlbumArtists:
            @escaping @MainActor (
                [BridgeArtistAssignment]
            ) async -> Void = { _ in }
    ) {
        self.setField = setField
        self.setAlbumArtists = setAlbumArtists
    }

    static func binding(_ form: Binding<BridgeRawReleaseEdit>) -> Self {
        Self(
            setField: { field, value in
                switch field {
                case .albumTitle: form.wrappedValue.albumTitle = value
                case .albumYear: form.wrappedValue.albumYear = value
                case .pressingYear: form.wrappedValue.pressing.year = value
                case .format: form.wrappedValue.pressing.format = value
                case .label: form.wrappedValue.pressing.label = value
                case .catalogNumber:
                    form.wrappedValue.pressing.catalogNumber = value
                case .country: form.wrappedValue.pressing.country = value
                case .barcode: form.wrappedValue.pressing.barcode = value
                }
            },
            setAlbumArtists: { assignments in
                form.wrappedValue.albumArtistAssignments = assignments
            }
        )
    }
}

/// The sizes the release header is laid out to, shared by the surfaces that
/// draw it — the import pane's card and the library's edit sheet — and by
/// the covers they hand it.
enum ReleaseMetadataLayout {
    /// The cover beside the album identity. Big enough to read the artwork
    /// as artwork; the heading beside it is sized to match.
    static let coverSize: CGFloat = 132
    /// Between the cover and the identity column.
    static let coverSpacing: CGFloat = 16
    /// Between the header's blocks: the cover row, what the folder states,
    /// the release facts, and whatever the surface stacks after them.
    static let blockSpacing: CGFloat = 14
}

/// The shared editable release header: the cover beside the album identity,
/// then what the folder itself states, then the release facts, each a block
/// of its own the full width of the surface. Its callers supply the cover and
/// the two folder slots so candidate and persisted-release ownership never
/// leaks into this component.
struct ReleaseMetadataHeader<Cover: View, AudioFacts: View, FolderFacts: View>:
    View
{
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    @ViewBuilder
    let cover: () -> Cover
    /// What the audio behind the release is — its codec, rate and depth —
    /// as the last line of the identity column, under the artist. Empty
    /// where nothing has read the files.
    @ViewBuilder
    let audioFacts: () -> AudioFacts
    /// The names the object carries and what the rip databases said about
    /// its bits: a block of its own under the cover row, above the release
    /// facts. Empty where there is no folder behind the release.
    @ViewBuilder
    let folderFacts: () -> FolderFacts

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            HStack(alignment: .top, spacing: ReleaseMetadataLayout.coverSpacing)
            {
                cover()
                    .frame(
                        width: ReleaseMetadataLayout.coverSize,
                        height: ReleaseMetadataLayout.coverSize
                    )
                ReleaseAlbumIdentityEditor(
                    values: values,
                    writer: writer,
                    editingCommands: editingCommands,
                    audioFacts: audioFacts
                )
            }
            folderFacts()
            ReleasePressingFieldsGrid(
                values: values,
                writer: writer,
                editingCommands: editingCommands
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Album identity rendered as a document heading that becomes editable on
/// hover and focus: the title, the artist line, and the audio facts.
struct ReleaseAlbumIdentityEditor<AudioFacts: View>: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    @ViewBuilder
    let audioFacts: () -> AudioFacts

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            CommittedTextField(
                placeholder: String(localized: "Album title"),
                value: values.albumTitle,
                chrome: .inline,
                font: .system(size: 22, weight: .semibold),
                editingCommands: editingCommands,
                onCommit: { await writer.setField(.albumTitle, $0) },
            )
            HStack(alignment: .center, spacing: 6) {
                ArtistAssignmentsField(
                    assignments: values.albumArtistAssignments,
                    placeholder: String(localized: "Album artist"),
                    onChange: { assignments in
                        Task { await writer.setAlbumArtists(assignments) }
                    },
                )
                .font(.system(size: 14))
                .foregroundStyle(.secondary)
                .modifier(FieldChrome(focused: false, style: .inline))
                Text(verbatim: "\u{00b7}")
                    .font(.system(size: 14))
                    .foregroundStyle(.quaternary)
                CommittedTextField(
                    placeholder: String(localized: "Year"),
                    value: values.albumYear,
                    chrome: .inline,
                    font: .system(size: 13),
                    editingCommands: editingCommands,
                    onCommit: { await writer.setField(.albumYear, $0) },
                )
                .foregroundStyle(.secondary)
                .frame(width: 72)
            }
            audioFacts()
                .padding(.horizontal, FieldChrome.inlineHorizontalPadding)
        }
        .padding(.leading, -FieldChrome.inlineHorizontalPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Editable release facts as one column of labelled fields under a ruled
/// RELEASE header, used by Import and the persisted release editor.
struct ReleasePressingFieldsGrid: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands

    static let labelWidth: CGFloat = 64
    static let labelGap: CGFloat = 14
    /// Between one row's text and the next. The fields carry the inline
    /// chrome's vertical padding inside their rows, so the grid's own
    /// spacing is what is left of the gap once that padding is counted.
    static let rowSpacing: CGFloat = 8
    /// What an empty field is drawn at, so there is something to click into;
    /// a filled field is as wide as its value.
    static let emptyValueWidth: CGFloat = 96

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: String(localized: "Release"), ruled: true)
            Grid(
                alignment: .leadingFirstTextBaseline,
                horizontalSpacing: Self.labelGap
                    - FieldChrome.inlineHorizontalPadding,
                verticalSpacing: Self.rowSpacing
                    - 2 * FieldChrome.inlineVerticalPadding
            ) {
                GridRow {
                    field(
                        .pressingYear,
                        label: String(localized: "Year"),
                        text: values.pressing.year
                    )
                }
                GridRow {
                    field(
                        .format,
                        label: coreString("core.release.media"),
                        text: values.pressing.format
                    )
                }
                GridRow {
                    field(
                        .label,
                        label: String(localized: "Label"),
                        text: values.pressing.label
                    )
                }
                GridRow {
                    field(
                        .country,
                        label: String(localized: "Country"),
                        text: values.pressing.country
                    )
                }
                GridRow {
                    field(
                        .catalogNumber,
                        label: String(localized: "Catalog"),
                        text: values.pressing.catalogNumber,
                        monospaced: true
                    )
                }
                GridRow {
                    field(
                        .barcode,
                        label: String(localized: "Barcode"),
                        text: values.pressing.barcode,
                        monospaced: true
                    )
                }
            }
        }
    }

    /// One row: the label right-aligned in its column, the field as wide as
    /// its value. An empty field is drawn wider than its dash so there is
    /// something to click into.
    @ViewBuilder
    private func field(
        _ field: BridgeCandidateEditField,
        label: String,
        text: String,
        monospaced: Bool = false
    ) -> some View {
        Text(label)
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: Self.labelWidth, alignment: .trailing)
        CommittedTextField(
            placeholder: "\u{2014}",
            value: text,
            monospaced: monospaced,
            chrome: .inline,
            font: .system(size: 12.5),
            placeholderRole: .emptyMark,
            editingCommands: editingCommands,
            onCommit: { await writer.setField(field, $0) },
        )
        .fixedSize(horizontal: true, vertical: false)
        .frame(
            minWidth: text.isEmpty ? Self.emptyValueWidth : nil,
            alignment: .leading
        )
    }
}

extension BridgeArtistAssignment {
    var displayName: String {
        switch self {
        case .existing(let artist): artist.name
        case .new(let seed): seed.name
        }
    }

    var identityLabel: String {
        switch self {
        case .existing: String(localized: "Library")
        case .new: String(localized: "New")
        }
    }

    var isNew: Bool {
        if case .new = self { return true }
        return false
    }
}

/// What a closed artist field says about a whole set of assignments: the names
/// as one localized list, and one badge for how the set stands to the library
/// — every name already in it, every name new to it, or how many of them are
/// new. One assignment summarizes to that assignment's own name and badge.
struct ArtistAssignmentsSummary: Equatable {
    let names: String
    let identityLabel: String

    /// `nil` when nothing is assigned: the field shows its placeholder, which
    /// is not a summary of anything.
    init?(assignments: [BridgeArtistAssignment]) {
        guard !assignments.isEmpty else { return nil }
        names = ListFormatter.localizedString(
            byJoining: assignments.map(\.displayName)
        )
        let newCount = assignments.filter(\.isNew).count
        identityLabel =
            if newCount == 0 {
                String(localized: "Library")
            }
            else if newCount == assignments.count {
                String(localized: "New")
            }
            else {
                String(localized: "\(newCount) new")
            }
    }
}

/// The capsule naming how a name — or a whole field's worth of them — stands
/// to the library.
struct ArtistIdentityBadge: View {
    let label: String

    var body: some View {
        Text(label)
            .font(.system(size: 9.5, weight: .medium))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(.quaternary, in: Capsule())
            .fixedSize()
    }
}

struct ArtistAssignmentLabel: View {
    let assignment: BridgeArtistAssignment

    var body: some View {
        HStack(spacing: 5) {
            Text(assignment.displayName)
                .lineLimit(1)
                .truncationMode(.middle)
            ArtistIdentityBadge(label: assignment.identityLabel)
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

struct ArtistAssignmentsField: View {
    /// How wide the closed field draws. On a header line it is as wide as the
    /// value it shows, so the fields after it stay beside it; in a table it
    /// spans its column, so the chevrons line up down the rows.
    enum Width {
        case value
        case column
    }

    let assignments: [BridgeArtistAssignment]
    let placeholder: String
    var width: Width = .value
    var inheritsAlbumArtists = false
    var onUseAlbumArtists: (() -> Void)?
    let onChange: ([BridgeArtistAssignment]) -> Void

    @Environment(Library.self)
    private var library
    @Environment(UiStore.self)
    private var uiStore
    @State
    private var isPresented = false
    @State
    private var query = ""
    @State
    private var results: [BridgeArtistSearchResult] = []
    @State
    private var isSearching = false
    @State
    private var errorMessage: String?

    var body: some View {
        Button {
            isPresented = true
        } label: {
            HStack(spacing: 6) {
                fieldValue
                if width == .column {
                    Spacer(minLength: 0)
                }
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusable()
        .popover(isPresented: $isPresented, arrowEdge: .bottom) {
            editor
                .frame(width: 320)
                .padding(12)
                .background { PopoverBehavior() }
        }
    }

    @ViewBuilder
    private var fieldValue: some View {
        if inheritsAlbumArtists {
            Text("Album artist").foregroundStyle(.tertiary)
        }
        else if let summary = ArtistAssignmentsSummary(assignments: assignments)
        {
            HStack(spacing: 5) {
                Text(summary.names)
                    .lineLimit(1)
                    .truncationMode(.tail)
                ArtistIdentityBadge(label: summary.identityLabel)
            }
        }
        else {
            Text(placeholder).foregroundStyle(.tertiary)
        }
    }

    private var editor: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let onUseAlbumArtists {
                Button("Album artist", action: onUseAlbumArtists)
                    .buttonStyle(.link)
            }
            ForEach(Array(assignments.enumerated()), id: \.offset) {
                index,
                assignment in
                HStack(spacing: 8) {
                    ArtistAssignmentLabel(assignment: assignment)
                    Spacer(minLength: 0)
                    Button {
                        var next = assignments
                        next.remove(at: index)
                        onChange(next)
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.borderless)
                    .accessibilityLabel(Text("Remove"))
                }
            }
            HStack(spacing: 8) {
                TextField("Search", text: $query)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(addTypedArtist)
                Button("Add", action: addTypedArtist)
                    .disabled(trimmedQuery.isEmpty)
            }
            if isSearching {
                ProgressView().controlSize(.small)
            }
            if let errorMessage {
                Text(errorMessage).font(.caption).foregroundStyle(.red)
            }
            ForEach(results, id: \.artist.artistId) { result in
                VStack(alignment: .leading, spacing: 1) {
                    Button {
                        onChange(
                            assignments + [.existing(artist: result.artist)]
                        )
                        query = ""
                    } label: {
                        ArtistSearchResultLabel(artist: result.artist)
                    }
                    .buttonStyle(.plain)
                    Button("View in Library") {
                        isPresented = false
                        uiStore.navigateToArtist(result.artist.artistId)
                    }
                    .buttonStyle(.link)
                    .font(.caption)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: query) { await search() }
    }

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func addTypedArtist() {
        guard !trimmedQuery.isEmpty else { return }
        onChange(
            assignments + [
                .new(
                    seed: BridgeNewArtistSeed(
                        name: query,
                        sortName: nil,
                        musicbrainzArtistId: nil,
                        discogsArtistId: nil
                    )
                )
            ]
        )
        query = ""
    }

    @MainActor
    private func search() async {
        errorMessage = nil
        guard !trimmedQuery.isEmpty else {
            results = []
            isSearching = false
            return
        }
        do {
            try await Task.sleep(for: .milliseconds(200))
            isSearching = true
            results = try await library.searchArtists(query)
            isSearching = false
        }
        catch is CancellationError {
            isSearching = false
        }
        catch {
            isSearching = false
            results = []
            errorMessage = error.displayLine
        }
    }
}

#if DEBUG
    #Preview("Release metadata header") {
        @Previewable
        @State
        var form = PreviewData.editMetadataDraft(trackCount: 3)
        ReleaseMetadataHeader(
            values: form,
            writer: .binding($form),
            editingCommands: EditingCommitCommands(),
            cover: {
                ImageView(
                    imageRef: nil,
                    pointSize: ReleaseMetadataLayout.coverSize
                )
                .clipShape(RoundedRectangle(cornerRadius: 8))
            },
            audioFacts: { EmptyView() },
            folderFacts: { EmptyView() }
        )
        .padding(24)
        .frame(width: 900, height: 480)
        .background(Theme.background)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(ImageStore.stub())
        .environment(UiStore())
    }

    #Preview("Release metadata header, a compilation's artists") {
        @Previewable
        @State
        var form = PreviewData.manyAlbumArtistsDraft()
        ReleaseMetadataHeader(
            values: form,
            writer: .binding($form),
            editingCommands: EditingCommitCommands(),
            cover: {
                ImageView(
                    imageRef: nil,
                    pointSize: ReleaseMetadataLayout.coverSize
                )
                .clipShape(RoundedRectangle(cornerRadius: 8))
            },
            audioFacts: { EmptyView() },
            folderFacts: { EmptyView() }
        )
        .padding(24)
        .frame(width: 900, height: 480)
        .background(Theme.background)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(ImageStore.stub())
        .environment(UiStore())
    }
#endif
