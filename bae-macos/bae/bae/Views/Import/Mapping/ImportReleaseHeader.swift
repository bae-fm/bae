import BaeKit
import SwiftUI

/// The commit controls the card carries once there is something to commit:
/// storage, the Import action, and what is still unanswered. Nothing here
/// disables the commit — the count is a statement, and the one refusal left
/// in the whole import is audio that will not decode, which core raises.
struct ImportCommitControls {
    let unansweredCount: Int
    /// Routes the running import's progress to the leaf line that draws it.
    let candidateKey: String
    /// Where the candidate's import stands, as its row places it.
    let importStatus: BridgeTriageImportStatus?
    let storageCloud: Binding<Bool>
    let storagePinned: Binding<Bool>
    let actions: ImportCommitActions
}

struct ImportReleaseSourceActions {
    let findOnline: () -> Void
    let useFileTags: () -> Void
    let clearMetadata: () -> Void
}

/// The album as a document heading: the title large, the artist and year
/// under it, the folder's audio facts under those. Every line is the field it
/// reads — the chrome arrives on hover and focus, so at rest it reads as a
/// heading and under the pointer it reads as the editor it is.
struct ImportAlbumIdentityEditor: View {
    let values: BridgeRawReleaseEdit
    let summary: ImportReleaseSummary
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            CommittedTextField(
                placeholder: String(localized: "Album title"),
                value: values.albumTitle,
                boxed: false,
                font: .system(size: 24, weight: .semibold),
                editingCommands: editingCommands,
                onCommit: {
                    await writer.setField(.albumTitle, $0)
                },
            )
            HStack(alignment: .firstTextBaseline, spacing: 6) {
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
                .modifier(FieldChrome(focused: false, boxed: false))
                Text(verbatim: "\u{00b7}")
                    .font(.system(size: 14))
                    .foregroundStyle(.quaternary)
                CommittedTextField(
                    placeholder: String(localized: "Year"),
                    value: values.albumYear,
                    boxed: false,
                    font: .system(size: 13),
                    editingCommands: editingCommands,
                    onCommit: {
                        await writer.setField(.albumYear, $0)
                    },
                )
                .foregroundStyle(.secondary)
                .frame(width: 72)
                ImportReleaseContextView(summary: summary)
            }
            if let sourceAudio = summary.sourceAudio {
                ImportSourceAudioSummaryView(sourceAudio: sourceAudio)
                    .padding(.horizontal, FieldChrome.horizontalPadding)
            }
        }
        // The fields are borderless, so their text sits a chrome-pad in from
        // the column's edge. Pull the block back by that pad: the heading text
        // lines up with the Release eyebrow and labels under it, and the chrome
        // that appears on hover reaches into the gutter beside the cover.
        .padding(.leading, -FieldChrome.horizontalPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The pressing fields of the release being written, two to a row under a
/// ruled Release heading. They are always in view: a pressing is half of what
/// the import commits, and a fold hid the half most often worth checking.
///
/// The grid is a fact sheet: fixed label and value columns that hug the
/// leading edge, rows one text line apart, no rules between them. An empty
/// value is an em dash in the tertiary color. The field chrome arrives on
/// hover and focus — at rest the grid reads as facts, under the pointer as
/// the form.
struct ImportReleaseFieldsGrid: View {
    let values: BridgeRawReleaseEdit
    let writer: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands

    static let labelWidth: CGFloat = 104
    static let valueWidth: CGFloat = 150
    /// From the end of a value to the start of the next label.
    static let columnGap: CGFloat = 36
    /// From the end of a label to the start of its value.
    static let labelGap: CGFloat = 12
    static let rowSpacing: CGFloat = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            FormSectionHeader(title: String(localized: "Release"), ruled: true)
            // The value text sits a chrome-pad inside its borderless field on
            // every side, so the grid's gaps are what remain of the stated
            // distances once that pad is taken off.
            Grid(
                alignment: .leadingFirstTextBaseline,
                horizontalSpacing: Self.columnGap
                    - FieldChrome.horizontalPadding,
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
                        label: String(localized: "Catalog number"),
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
            spacing: Self.labelGap - FieldChrome.horizontalPadding
        ) {
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: Self.labelWidth, alignment: .leading)
            CommittedTextField(
                placeholder: "\u{2014}",
                value: text,
                monospaced: monospaced,
                boxed: false,
                font: .system(
                    size: 12.5,
                    design: monospaced ? .monospaced : .default
                ),
                placeholderStyle: .tertiary,
                editingCommands: editingCommands,
                onCommit: { await writer.setField(field, $0) },
            )
            .frame(width: Self.valueWidth + FieldChrome.horizontalPadding * 2)
            // Rows sit one text line apart: the chrome's vertical pad is
            // taken back out of the layout and drawn into the row gap when
            // the field is hovered or focused.
            .padding(.vertical, -FieldChrome.verticalPadding)
        }
    }
}

/// The editable metadata draft card: where the draft's metadata comes from,
/// then the cover beside the album heading and its release fields, then the
/// commit row once there is something to commit.
struct ImportReleaseHeader: View {
    let releaseSummary: ImportReleaseSummary
    /// Whether a read is in flight — the source controls wait for it rather
    /// than the card being replaced by a placeholder.
    let isReading: Bool
    let coverContent: ImageContent?
    /// Whether there is any artwork to pick from — the release's images or the
    /// folder's. Without one, the cover well says so instead of inviting a
    /// pick.
    let hasCoverOptions: Bool
    /// `nil` when there is no release to edit.
    let editValues: BridgeRawReleaseEdit?
    /// Where a typed field's value goes.
    let editActions: ReleaseFieldWriter
    let editingCommands: EditingCommitCommands
    /// The commit row at the card's foot. `nil` while there is nothing to
    /// commit.
    let commit: ImportCommitControls?
    let sourceActions: ImportReleaseSourceActions
    let localCoverSelections: [String: BridgeCoverSelection]
    let onEditCover: () -> Void
    let onSelectCover: (BridgeCoverSelection) -> Void

    /// The cover the card leads with. Big enough to read the artwork as
    /// artwork — the cover is the thing being confirmed, and the heading
    /// beside it is sized to match.
    static let coverSize: CGFloat = 200

    @Environment(ConfigStore.self)
    private var configStore
    @State
    private var confirmsClear = false
    @State
    private var coverDropTargeted = false
    @State
    private var coverHovering = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            sourceActionRow
            HStack(alignment: .top, spacing: 24) {
                cover
                VStack(alignment: .leading, spacing: 22) {
                    if let editValues {
                        ImportAlbumIdentityEditor(
                            values: editValues,
                            summary: releaseSummary,
                            writer: editActions,
                            editingCommands: editingCommands
                        )
                        ImportReleaseFieldsGrid(
                            values: editValues,
                            writer: editActions,
                            editingCommands: editingCommands
                        )
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            if let commit {
                commitRow(commit)
            }
        }
        .padding(16)
        .formGroupCard()
        .confirmationDialog(
            "Clear metadata?",
            isPresented: $confirmsClear,
            titleVisibility: .visible
        ) {
            Button("Clear metadata", role: .destructive) {
                sourceActions.clearMetadata()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                "The candidate files and mapping choices will remain unchanged."
            )
        }
    }

    /// Storage, the unanswered tally when there is one, and the Import action
    /// — the commit lives on the card that states what will be committed.
    private func commitRow(_ commit: ImportCommitControls) -> some View {
        HStack(alignment: .center, spacing: 16) {
            if commit.unansweredCount > 0 {
                Text(
                    coreString(
                        "ui.import.commit.unanswered",
                        commit.unansweredCount
                    )
                )
                .font(.system(size: 11.5))
                .foregroundStyle(.orange)
            }
            Spacer(minLength: 12)
            if !commitSettled(commit), configStore.config.hasCloudHome {
                HStack(spacing: 10) {
                    ImportCheckboxToggle(
                        "Cloud",
                        isOn: commit.storageCloud
                    )
                    if commit.storageCloud.wrappedValue {
                        ImportCheckboxToggle(
                            "Pinned",
                            isOn: commit.storagePinned
                        )
                    }
                }
                .fixedSize()
            }
            ImportConfirmationCardAction(
                importStatus: commit.importStatus,
                candidateKey: commit.candidateKey,
                onConfirmImport: commit.actions.confirmImport,
                onViewInLibrary: commit.actions.viewInLibrary,
            )
        }
    }

    /// Whether the import already ran or is running — the storage choice is
    /// spent then, so its toggles leave the row.
    private func commitSettled(_ commit: ImportCommitControls) -> Bool {
        switch commit.importStatus {
        case .importing, .complete: return true
        case .error, nil: return false
        }
    }

    /// Where the draft's metadata comes from. One constant set of controls in
    /// every draft state: both sources replace the same draft, so neither is
    /// promoted over the other, and clearing sits behind the ellipsis.
    private var sourceActionRow: some View {
        HStack(spacing: 8) {
            Button("Find release…") {
                sourceActions.findOnline()
            }
            .buttonStyle(.bordered)
            Button("Use file metadata") {
                sourceActions.useFileTags()
            }
            .buttonStyle(.bordered)
            clearMetadataMenu
            Spacer(minLength: 0)
        }
        .disabled(isReading)
    }

    private var clearMetadataMenu: some View {
        Menu {
            Button("Clear metadata", role: .destructive) {
                confirmsClear = true
            }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(width: 28, height: 22)
                .contentShape(Rectangle())
                .accessibilityLabel(Text("Clear metadata"))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .background(
            .white.opacity(0.06),
            in: RoundedRectangle(cornerRadius: 6)
        )
    }

    /// The cover, or the well it goes in. The well invites the two ways a
    /// cover arrives — dragging one of the folder's images onto it, or picking
    /// from the release's and the folder's — and says when there is neither.
    private var cover: some View {
        Group {
            if let coverContent {
                ImageView(content: coverContent, pointSize: Self.coverSize)
            }
            else {
                artworkWell
            }
        }
        .frame(width: Self.coverSize, height: Self.coverSize)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(alignment: .topTrailing) {
            if coverContent != nil, hasCoverOptions {
                Image(systemName: "pencil")
                    .font(.caption2)
                    .foregroundStyle(.white)
                    .padding(3)
                    .background(.black.opacity(0.5))
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .padding(4)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            if hasCoverOptions {
                onEditCover()
            }
        }
        .onHover { coverHovering = $0 }
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .stroke(
                    Theme.accent,
                    lineWidth: coverDropTargeted ? 3 : 0
                )
        }
        .dropDestination(for: String.self) { fileIds, _ in
            guard let fileId = fileIds.first,
                let selection = localCoverSelections[fileId]
            else { return false }
            onSelectCover(selection)
            return true
        } isTargeted: {
            coverDropTargeted = $0
        }
    }

    private var artworkWell: some View {
        let inviting = hasCoverOptions && coverHovering
        return VStack(spacing: 4) {
            if hasCoverOptions {
                Text("Add artwork")
                    .font(.system(size: 12, weight: .semibold))
                Text("Drag an image here, or click to choose")
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            else {
                Text("No artwork")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .frame(width: Self.coverSize, height: Self.coverSize)
        .background(
            inviting ? Theme.accent.opacity(0.06) : .white.opacity(0.03)
        )
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [4, 4]))
                .foregroundStyle(
                    inviting
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(.white.opacity(0.16))
                )
        }
    }
}

#if DEBUG

    #Preview("Import release header") {
        ImportReleaseHeader(
            releaseSummary: ImportReleaseSummary(
                candidate: PreviewData.mappingCandidate,
                editValues: PreviewData.confirmEditValues
            ),
            isReading: false,
            coverContent: nil,
            hasCoverOptions: true,
            editValues: PreviewData.confirmEditValues,
            editActions: ReleaseFieldWriter { _, _ in },
            editingCommands: EditingCommitCommands(),
            commit: nil,
            sourceActions: ImportReleaseSourceActions(
                findOnline: {},
                useFileTags: {},
                clearMetadata: {}
            ),
            localCoverSelections: [:],
            onEditCover: {},
            onSelectCover: { _ in },
        )
        .padding(24)
        .frame(width: 900, height: 420)
        .importPreviewEnvironment()
        .environment(Library.stub())
        .candidateReaderPreviewEnvironment()
    }

#endif
