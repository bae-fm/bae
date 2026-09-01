import BaeKit
import SwiftUI

/// Where one field's typed value goes.
///
/// The library's release editor holds its form in memory and saves it whole,
/// so its fields write into a binding. The import pane holds nothing: each
/// field is a row under the candidate, and typing in one writes that row. Both
/// hand this the same album and pressing fields.
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

    /// Write into a form held in memory.
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

/// The album and pressing fields of a release, as one form.
///
/// It reads values and reports edits; where those values live is the caller's.
struct ReleaseFieldsForm: View {
    enum Section: Hashable {
        case album
        case pressing
    }

    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let sections: Set<Section>
    let showsSectionHeaders: Bool
    var editingCommands: EditingCommitCommands?

    init(
        values: BridgeRawReleaseEdit,
        writer: ReleaseFieldWriter,
        sections: Set<Section> = [.album, .pressing],
        showsSectionHeaders: Bool = true,
        editingCommands: EditingCommitCommands? = nil
    ) {
        self.values = values
        self.writer = writer
        self.sections = sections
        self.showsSectionHeaders = showsSectionHeaders
        self.editingCommands = editingCommands
    }

    /// A form over a value held in memory — the library's release editor.
    init(form: Binding<BridgeRawReleaseEdit>) {
        values = form.wrappedValue
        writer = .binding(form)
        sections = [.album, .pressing]
        showsSectionHeaders = true
        editingCommands = nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            if sections.contains(.album) {
                albumGroup
            }
            if sections.contains(.pressing) {
                pressingGroup
            }
        }
    }

    private func row(
        _ field: BridgeCandidateEditField,
        label: String,
        hint: String? = nil,
        placeholder: String,
        text: String,
        width: FieldWidth,
        monospaced: Bool = false
    ) -> FieldRow {
        FieldRow(
            label: label,
            hint: hint,
            placeholder: placeholder,
            text: text,
            onCommit: { await writer.setField(field, $0) },
            width: width,
            monospaced: monospaced,
        )
    }

    private var albumGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            if showsSectionHeaders {
                FormSectionHeader(title: String(localized: "Album"))
            }
            VStack(spacing: 0) {
                fieldRow(
                    row(
                        .albumTitle,
                        label: String(localized: "Title"),
                        placeholder: String(localized: "Album title"),
                        text: values.albumTitle,
                        width: .long,
                    )
                )
                Rectangle()
                    .fill(.white.opacity(0.07))
                    .frame(height: 1)
                albumArtistsRow
                Rectangle()
                    .fill(.white.opacity(0.07))
                    .frame(height: 1)
                fieldRow(
                    row(
                        .albumYear,
                        label: String(localized: "Year"),
                        placeholder: String(localized: "Year"),
                        text: values.albumYear,
                        width: .short,
                        monospaced: true,
                    )
                )
            }
            .formGroupCard()
        }
    }

    private var albumArtistsRow: some View {
        HStack(spacing: 16) {
            Text("Artist")
                .font(.system(size: 13))
                .frame(width: 150, alignment: .leading)
            ArtistAssignmentsField(
                assignments: values.albumArtistAssignments,
                placeholder: String(localized: "Album artist"),
                onChange: { assignments in
                    Task { await writer.setAlbumArtists(assignments) }
                },
            )
            .frame(maxWidth: FieldWidth.long.maxWidth)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
    }

    private var pressingGroup: some View {
        groupCard(
            title:
                showsSectionHeaders
                ? String(localized: "Release pressing") : nil,
            rows: [
                row(
                    .pressingYear,
                    label: String(localized: "Year"),
                    placeholder: String(localized: "Year"),
                    text: values.pressing.year,
                    width: .short,
                    monospaced: true,
                ),
                row(
                    .format,
                    label: coreString("core.release.media"),
                    placeholder: coreString("core.release.media"),
                    text: values.pressing.format,
                    width: .short,
                ),
                row(
                    .label,
                    label: String(localized: "Label"),
                    placeholder: String(localized: "Label"),
                    text: values.pressing.label,
                    width: .long,
                ),
                row(
                    .country,
                    label: String(localized: "Country"),
                    placeholder: String(localized: "Country"),
                    text: values.pressing.country,
                    width: .short,
                ),
                row(
                    .catalogNumber,
                    label: String(localized: "Catalog number"),
                    placeholder: String(localized: "Catalog number"),
                    text: values.pressing.catalogNumber,
                    width: .medium,
                ),
                row(
                    .barcode,
                    label: String(localized: "Barcode"),
                    placeholder: String(localized: "Barcode"),
                    text: values.pressing.barcode,
                    width: .medium,
                    monospaced: true,
                ),
            ],
        )
    }

    /// A titled inset card of label-left / value-right rows separated by
    /// hairlines.
    private func groupCard(title: String?, rows: [FieldRow]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if let title {
                FormSectionHeader(title: title)
            }
            VStack(spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.label) {
                    index,
                    row in
                    fieldRow(row)
                    if index < rows.count - 1 {
                        Rectangle()
                            .fill(.white.opacity(0.07))
                            .frame(height: 1)
                    }
                }
            }
            .formGroupCard()
        }
    }

    private func fieldRow(_ row: FieldRow) -> some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 1) {
                Text(row.label)
                    .font(.system(size: 13))
                if let hint = row.hint {
                    Text(hint)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.quaternary)
                }
            }
            .frame(width: 150, alignment: .leading)
            CommittedTextField(
                placeholder: row.placeholder,
                value: row.text,
                monospaced: row.monospaced,
                editingCommands: editingCommands,
                onCommit: row.onCommit,
            )
            .frame(maxWidth: row.width.maxWidth)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
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

/// One assigned artist's name and its source-owned identity kind. The badge
/// switches on the bridge enum itself, so equal names never imply linkage.
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

/// The display name for an existing artist search result.
struct ArtistSearchResultLabel: View {
    let artist: BridgeExistingArtist

    var body: some View {
        Text(artist.name)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Ordered artist choices shared by the library editor and import mapping.
/// Existing artists are selected from the library; typed names remain explicit
/// new-artist seeds. The compact field opens a popover so track-row dimensions
/// never change while searching.
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
            Text("Album artist")
                .foregroundStyle(.tertiary)
        }
        else if assignments.isEmpty {
            Text(placeholder)
                .foregroundStyle(.tertiary)
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
                ProgressView()
                    .controlSize(.small)
            }
            if let errorMessage {
                Text(errorMessage)
                    .font(.caption)
                    .foregroundStyle(.red)
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
        .task(id: query) {
            await search()
        }
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
    #Preview("Release fields") {
        @Previewable
        @State
        var form = PreviewData.editMetadataDraft(trackCount: 3)
        ScrollView {
            ReleaseFieldsForm(form: $form)
                .padding(20)
        }
        .frame(width: 640, height: 520)
        .background(Theme.background)
        .preferredColorScheme(.dark)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(UiStore())
    }
#endif
