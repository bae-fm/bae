import BaeKit
import SwiftUI

/// Where one release field's settled value goes: a candidate draft persists
/// each field, a persisted-release session updates its working form.
struct ReleaseFieldWriter {
    let setField: @MainActor (BridgeCandidateEditField, String) async -> Void
    /// Every label row, after one is typed into, added or removed.
    let setLabels: @MainActor ([BridgeRawLabelEdit]) async -> Void
    /// A choice of what the pressing is, from one of the form's pickers.
    let setPressingFact: @MainActor (BridgePressingFactEdit) async -> Void
    let setAlbumArtists: @MainActor ([BridgeArtistAssignment]) async -> Void

    init(
        setField:
            @escaping @MainActor (
                BridgeCandidateEditField, String
            ) async -> Void,
        setLabels:
            @escaping @MainActor (
                [BridgeRawLabelEdit]
            ) async -> Void = { _ in },
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
        self.setLabels = setLabels
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
                case .barcode: form.wrappedValue.pressing.barcode = value
                }
            },
            setLabels: { labels in
                form.wrappedValue.pressing.labels = labels
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

/// The release header's sizes, shared with the surfaces that draw it and the
/// covers they pass in.
enum ReleaseMetadataLayout {
    /// The cover beside the album identity.
    static let coverSize: CGFloat = 132
    /// Between the cover and the identity column.
    static let coverSpacing: CGFloat = 16
    /// Between the header's blocks.
    static let blockSpacing: CGFloat = 14
}

/// The shared editable release header: the cover beside the album identity,
/// then the release facts. Callers supply the cover and the audio facts.
struct ReleaseMetadataHeader<Cover: View, AudioFacts: View>:
    View
{
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    @ViewBuilder
    let cover: () -> Cover
    /// The audio's codec, rate and depth as the identity column's last line;
    /// empty where nothing has read the files.
    @ViewBuilder
    let audioFacts: () -> AudioFacts

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            HStack(alignment: .top, spacing: ReleaseMetadataLayout.coverSpacing)
            {
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

/// The album identity drawn as a heading that is editable in place.
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
                font: ThemeText.title.nsFont,
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
            .themeText(.body)
            .foregroundStyle(.secondary)
            .modifier(FieldChrome(focused: false, style: .inline))
            // The album's original year; the pressing's year is under Release.
            CommittedTextField(
                placeholder: String(localized: "Album year"),
                value: values.albumYear,
                chrome: .inline,
                font: ThemeText.body.nsFont,
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

/// The editable release facts under a ruled Release header.
struct ReleasePressingFieldsGrid: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands

    static let labelWidth: CGFloat = 64
    static let labelGap: CGFloat = 14
    /// Between one row's text and the next, counting the fields' own inline
    /// padding.
    static let rowSpacing: CGFloat = 8
    /// An empty field's width, so there is something to click into.
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
                labelRows
                GridRow {
                    rowLabel(String(localized: "Country"))
                    ReleaseAreaPicker(
                        area: values.pressing.facts.area,
                        write: writer.setPressingFact
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

    /// One row per label, name beside number; a blank row when there is none.
    @ViewBuilder
    private var labelRows: some View {
        let rows =
            values.pressing.labels.isEmpty
            ? [BridgeRawLabelEdit(name: "", catalogNumber: "")]
            : values.pressing.labels
        ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
            GridRow {
                rowLabel(index == 0 ? String(localized: "Label") : "")
                HStack(spacing: 6) {
                    valueText(
                        row.name,
                        placeholder: "\u{2014}",
                        role: .emptyMark,
                        monospaced: false
                    ) { name in
                        var next = rows
                        next[index].name = name
                        await writer.setLabels(next)
                    }
                    valueText(
                        row.catalogNumber,
                        placeholder: String(localized: "Catalog #"),
                        role: .hint,
                        monospaced: true
                    ) { number in
                        var next = rows
                        next[index].catalogNumber = number
                        await writer.setLabels(next)
                    }
                    if rows.count > 1 || !row.name.isEmpty
                        || !row.catalogNumber.isEmpty
                    {
                        let named =
                            row.name.isEmpty ? row.catalogNumber : row.name
                        Button {
                            var next = rows
                            next.remove(at: index)
                            Task { await writer.setLabels(next) }
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.tertiary)
                        }
                        .buttonStyle(.plain)
                        .help(String(localized: "Remove \(named)"))
                        .accessibilityLabel(
                            String(localized: "Remove \(named)")
                        )
                    }
                    if index == rows.count - 1 {
                        Button {
                            let blank = BridgeRawLabelEdit(
                                name: "",
                                catalogNumber: ""
                            )
                            Task { await writer.setLabels(rows + [blank]) }
                        } label: {
                            Image(systemName: "plus")
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                        .help(String(localized: "Add label"))
                        .accessibilityLabel(String(localized: "Add label"))
                    }
                }
                .font(.system(size: 11))
            }
        }
    }

    /// A field as wide as its value, or `emptyValueWidth` when empty.
    private func valueText(
        _ text: String,
        placeholder: String,
        role: CommittedTextField.PlaceholderRole,
        monospaced: Bool,
        onCommit: @escaping @MainActor (String) async -> Void
    ) -> some View {
        CommittedTextField(
            placeholder: placeholder,
            value: text,
            monospaced: monospaced,
            chrome: .inline,
            font: ThemeText.body.nsFont,
            placeholderRole: role,
            editingCommands: editingCommands,
            onCommit: onCommit,
        )
        .fixedSize(horizontal: true, vertical: false)
        .frame(
            minWidth: text.isEmpty ? Self.emptyValueWidth : nil,
            alignment: .leading
        )
    }

    /// A row's label, right-aligned in its column.
    private func rowLabel(_ label: String) -> some View {
        Text(label)
            .themeText(.detail)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: Self.labelWidth, alignment: .trailing)
    }

    /// One row: the label right-aligned in its column, then the field.
    @ViewBuilder
    private func field(
        _ field: BridgeCandidateEditField,
        label: String,
        text: String,
        monospaced: Bool = false
    ) -> some View {
        rowLabel(label)
        valueText(
            text,
            placeholder: "\u{2014}",
            role: .emptyMark,
            monospaced: monospaced
        ) { await writer.setField(field, $0) }
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
    /// What the library holds for the artist credits on screen; empty (not read
    /// yet, or the read failed) shows no badge.
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

/// A closed artist field's text: the names as one localized list and, once
/// core has read them, one badge for the whole set.
struct ArtistAssignmentsSummary: Equatable {
    let names: String
    let identityLabel: String?

    /// `nil` when nothing is assigned, so the field shows its placeholder.
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
            .themeText(.chip)
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
    /// How wide the closed field draws: its value's width on a header line, the
    /// full column in a table so the chevrons line up.
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
                Text(errorMessage)
                    .themeText(.body)
                    .foregroundStyle(Theme.danger)
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
                    .themeText(.detail)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: query) { await search() }
    }

    /// The library artists a credit could be; picking one replaces the credit.
    private func choicesList(
        _ choices: [BridgeExistingArtist],
        replacing index: Int
    ) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("Which one?")
                .themeText(.detail)
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
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
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
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
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
