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

/// The shared editable release header: cover, album identity, source context,
/// and pressing facts. Its callers supply the cover and context so candidate
/// and persisted-release ownership never leaks into this component.
struct ReleaseMetadataHeader<Cover: View, Context: View, SourceAudio: View>:
    View
{
    static var coverSize: CGFloat { 200 }
    static var coverSpacing: CGFloat { 24 }

    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    @ViewBuilder
    let cover: () -> Cover
    @ViewBuilder
    let context: () -> Context
    @ViewBuilder
    let sourceAudio: () -> SourceAudio

    var body: some View {
        HStack(alignment: .top, spacing: Self.coverSpacing) {
            cover()
                .frame(width: Self.coverSize, height: Self.coverSize)
            VStack(alignment: .leading, spacing: 22) {
                ReleaseAlbumIdentityEditor(
                    values: values,
                    writer: writer,
                    editingCommands: editingCommands,
                    context: context,
                    sourceAudio: sourceAudio
                )
                ReleasePressingFieldsGrid(
                    values: values,
                    writer: writer,
                    editingCommands: editingCommands
                )
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// Album identity rendered as a document heading that becomes editable on
/// hover and focus.
struct ReleaseAlbumIdentityEditor<Context: View, SourceAudio: View>: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    @ViewBuilder
    let context: () -> Context
    @ViewBuilder
    let sourceAudio: () -> SourceAudio

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            CommittedTextField(
                placeholder: String(localized: "Album title"),
                value: values.albumTitle,
                chrome: .inline,
                font: .system(size: 24, weight: .semibold),
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
                .fixedSize(horizontal: true, vertical: false)
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
                context()
            }
            sourceAudio()
                .padding(.horizontal, FieldChrome.inlineHorizontalPadding)
        }
        .padding(.leading, -FieldChrome.inlineHorizontalPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Editable pressing facts in the compact two-column grid used by Import and
/// the persisted release editor.
struct ReleasePressingFieldsGrid: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands

    static let labelWidth: CGFloat = 64
    static let valueWidth: CGFloat = 150
    static let columnGap: CGFloat = 20
    static let labelGap: CGFloat = 12
    static let rowSpacing: CGFloat = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            FormSectionHeader(title: String(localized: "Release"), ruled: true)
            Grid(
                alignment: .leadingFirstTextBaseline,
                horizontalSpacing: Self.columnGap
                    - FieldChrome.inlineHorizontalPadding,
                verticalSpacing: Self.rowSpacing
            ) {
                GridRow {
                    field(
                        .pressingYear,
                        label: String(localized: "Year"),
                        text: values.pressing.year
                    )
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

    private func field(
        _ field: BridgeCandidateEditField,
        label: String,
        text: String,
        monospaced: Bool = false
    ) -> some View {
        HStack(
            alignment: .firstTextBaseline,
            spacing: Self.labelGap - FieldChrome.inlineHorizontalPadding
        ) {
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
                font: .system(
                    size: 12.5,
                    design: monospaced ? .monospaced : .default
                ),
                placeholderRole: .emptyMark,
                editingCommands: editingCommands,
                onCommit: { await writer.setField(field, $0) },
            )
            .frame(width: Self.valueWidth)
        }
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
}

struct ArtistAssignmentLabel: View {
    let assignment: BridgeArtistAssignment

    var body: some View {
        HStack(spacing: 5) {
            Text(assignment.displayName)
                .lineLimit(1)
                .truncationMode(.middle)
            Text(assignment.identityLabel)
                .font(.system(size: 9.5, weight: .medium))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(.quaternary, in: Capsule())
                .fixedSize()
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
    let assignments: [BridgeArtistAssignment]
    let placeholder: String
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
                Spacer(minLength: 0)
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
        else if assignments.isEmpty {
            Text(placeholder).foregroundStyle(.tertiary)
        }
        else {
            HStack(spacing: 8) {
                ForEach(Array(assignments.enumerated()), id: \.offset) {
                    _,
                    assignment in
                    ArtistAssignmentLabel(assignment: assignment)
                }
            }
            .lineLimit(1)
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
                ImageView(imageRef: nil, pointSize: 200)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
            },
            context: { EmptyView() },
            sourceAudio: { EmptyView() }
        )
        .padding(24)
        .frame(width: 900, height: 360)
        .background(Theme.background)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(UiStore())
    }
#endif
