import BaeKit
import SwiftUI

/// Where one release field's settled value goes. Candidate drafts persist each
/// field independently; persisted-release sessions update their working form.
struct ReleaseFieldWriter {
    let setField: @MainActor (BridgeCandidateEditField, String) async -> Void
    /// A choice of what the pressing is, from one of the form's pickers.
    let setPressingFact: @MainActor (BridgePressingFactEdit) async -> Void
    let setAlbumArtists: @MainActor ([BridgeArtistAssignment]) async -> Void

    init(
        setField:
            @escaping @MainActor (
                BridgeCandidateEditField, String
            ) async -> Void,
        setPressingFact:
            @escaping @MainActor (
                BridgePressingFactEdit
            ) async -> Void = { _ in },
        setAlbumArtists:
            @escaping @MainActor (
                [BridgeArtistAssignment]
            ) async -> Void = { _ in }
    ) {
        self.setField = setField
        self.setPressingFact = setPressingFact
        self.setAlbumArtists = setAlbumArtists
    }

    static func binding(_ form: Binding<BridgeRawReleaseEdit>) -> Self {
        Self(
            setField: { field, value in
                switch field {
                case .albumTitle: form.wrappedValue.albumTitle = value
                case .albumYear: form.wrappedValue.albumYear = value
                case .pressingYear: form.wrappedValue.pressing.year = value
                case .label: form.wrappedValue.pressing.label = value
                case .catalogNumber:
                    form.wrappedValue.pressing.catalogNumber = value
                case .barcode: form.wrappedValue.pressing.barcode = value
                }
            },
            setPressingFact: { fact in
                form.wrappedValue.pressing.facts = bridgeApplyPressingFact(
                    facts: form.wrappedValue.pressing.facts,
                    edit: fact
                )
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
struct ReleaseMetadataHeader<Cover: View, AudioFacts: View>:
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

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            HStack(alignment: .top, spacing: ReleaseMetadataLayout.coverSpacing)
            {
                // Clipped to the slot here, whatever the caller's cover does:
                // a non-square image fills the slot and is cut to it rather
                // than spilling over the identity column.
                cover()
                    .frame(
                        width: ReleaseMetadataLayout.coverSize,
                        height: ReleaseMetadataLayout.coverSize
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                ReleaseAlbumIdentityEditor(
                    values: values,
                    writer: writer,
                    editingCommands: editingCommands,
                    audioFacts: audioFacts
                )
            }
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
/// hover and focus: the title, the artist line, the album's year, and the
/// audio facts.
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
                font: .systemFont(ofSize: 22, weight: .semibold),
                editingCommands: editingCommands,
                onCommit: { await writer.setField(.albumTitle, $0) },
            )
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
            // The album's original year, a line of its own under the
            // artists and drawn a step fainter, as the library's album
            // heading draws it — the pressing's own year is the Release
            // facts' Year below.
            CommittedTextField(
                placeholder: String(localized: "Album year"),
                value: values.albumYear,
                chrome: .inline,
                font: .systemFont(ofSize: 13),
                textColor: .tertiaryLabelColor,
                editingCommands: editingCommands,
                onCommit: { await writer.setField(.albumYear, $0) },
            )
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
                    rowLabel(coreString("core.release.media"))
                    MediaCountsEditor(
                        media: values.pressing.facts.media,
                        write: writer.setPressingFact
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
                    rowLabel(String(localized: "Country"))
                    ReleaseAreaPicker(
                        area: values.pressing.facts.area,
                        write: writer.setPressingFact
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
                GridRow {
                    rowLabel(String(localized: "Status"))
                    ReleaseStatusPicker(
                        status: values.pressing.facts.status,
                        write: writer.setPressingFact
                    )
                }
                GridRow {
                    rowLabel(String(localized: "Packaging"))
                    PackagingPicker(
                        packaging: values.pressing.facts.packaging,
                        write: writer.setPressingFact
                    )
                }
                GridRow {
                    rowLabel(String(localized: "Details"))
                    DiscogsDetailsEditor(
                        details: values.pressing.facts.discogsDetails,
                        write: writer.setPressingFact
                    )
                }
            }
        }
    }

    /// A row's label, right-aligned in its column.
    private func rowLabel(_ label: String) -> some View {
        Text(label)
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: Self.labelWidth, alignment: .trailing)
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
        rowLabel(label)
        CommittedTextField(
            placeholder: "\u{2014}",
            value: text,
            monospaced: monospaced,
            chrome: .inline,
            font: .systemFont(ofSize: 12.5),
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
        case .picked(let artist): artist.name
        case .credit(let credit): credit.name
        }
    }
}

extension EnvironmentValues {
    // swiftui-environment-audit: optional
    /// What the library holds for the artist credits on screen, as core last
    /// read them: the import pane's live read, or the release editor's own.
    /// Rendered through `bridgeArtistStanding` / `bridgeArtistsStanding`.
    /// Empty is a real state, not a placeholder: nothing read yet, or a read
    /// that failed, shows no badge, which is what the editor sets itself.
    @Entry
    var artistResolutions: [BridgeResolvedCredit] = []
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

/// What a closed artist field says about a whole set of assignments: the names
/// as one localized list, and — once core has read how the set stands to the
/// library — one badge for it. One assignment summarizes to that assignment's
/// own name and badge.
struct ArtistAssignmentsSummary: Equatable {
    let names: String
    let identityLabel: String?

    /// `nil` when nothing is assigned: the field shows its placeholder, which
    /// is not a summary of anything.
    init?(
        assignments: [BridgeArtistAssignment],
        resolutions: [BridgeResolvedCredit]
    ) {
        guard !assignments.isEmpty else { return nil }
        names = ListFormatter.localizedString(
            byJoining: assignments.map(\.displayName)
        )
        identityLabel =
            bridgeArtistsStanding(
                assignments: assignments,
                resolutions: resolutions
            )?
            .label
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
    let standing: BridgeArtistStanding?

    var body: some View {
        HStack(spacing: 5) {
            Text(assignment.displayName)
                .lineLimit(1)
                .truncationMode(.middle)
            if let standing {
                ArtistIdentityBadge(label: standing.label)
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
    @Environment(\.artistResolutions)
    private var resolutions
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
        else if let summary = ArtistAssignmentsSummary(
            assignments: assignments,
            resolutions: resolutions
        ) {
            HStack(spacing: 5) {
                Text(summary.names)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if let label = summary.identityLabel {
                    ArtistIdentityBadge(label: label)
                }
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
                let standing = bridgeArtistStanding(
                    assignment: assignment,
                    resolutions: resolutions
                )
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        ArtistAssignmentLabel(
                            assignment: assignment,
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
                        .buttonStyle(.borderless)
                        .accessibilityLabel(Text("Remove"))
                    }
                    if case .choose(let choices) = standing {
                        choicesList(choices, replacing: index)
                    }
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
                            assignments + [.picked(artist: result.artist)]
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

    /// The library artists one credit could be, offered to pick from: picking
    /// one puts that library artist in the credit's place.
    private func choicesList(
        _ choices: [BridgeExistingArtist],
        replacing index: Int
    ) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("Which one?")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(choices, id: \.artistId) { choice in
                Button {
                    var next = assignments
                    next[index] = .picked(artist: choice)
                    onChange(next)
                } label: {
                    ArtistSearchResultLabel(artist: choice)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.leading, 12)
    }

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func addTypedArtist() {
        guard !trimmedQuery.isEmpty else { return }
        onChange(
            assignments + [
                .credit(
                    credit: BridgeArtistCredit(
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
            audioFacts: { EmptyView() }
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
            audioFacts: { EmptyView() }
        )
        .padding(24)
        .frame(width: 900, height: 480)
        .background(Theme.background)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(ImageStore.stub())
        .environment(UiStore())
    }
#endif
