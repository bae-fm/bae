import BaeKit
import SwiftUI

/// The one release editor, in the import pane and in the album's Edit
/// release sheet: Identify and the action row over the card, which holds the
/// cover, the album and pressing fields, the album picker and the records
/// with Unlink; then the release's images, its track table and its Files
/// list. The two hosts differ only in the action row.
struct MacReleaseEditor<Search: View>: View {
    let actionRow: ReleaseEditorActionRow
    let commands: ReleaseEditorCommands
    let card: ReleaseEditorCard
    let tracks: ReleaseEditorTracks
    let editingCommands: EditingCommitCommands
    /// Whether the person can edit and identify the release now; the
    /// records' links stay live either way.
    let editable: Bool
    /// Whether identification's results show in place of the card.
    let searching: Bool
    /// Identification's results, which replace the card while `searching`.
    @ViewBuilder
    let search: () -> Search

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            if searching {
                search()
            }
            else {
                ReleaseEditorCardView(
                    actionRow: actionRow,
                    commands: commands,
                    card: card,
                    editingCommands: editingCommands,
                    editable: editable
                )
            }
            if !tracks.images.isEmpty {
                MacImportImages(
                    images: tracks.images,
                    onOpen: tracks.onOpenImages
                )
            }
            if !tracks.sides.isEmpty {
                MacImportTable(
                    sides: tracks.sides,
                    previewingTarget: tracks.previewingTarget,
                    actions: tracks.actions,
                    editingCommands: editingCommands
                )
                .disabled(!editable)
            }
            if !tracks.files.isEmpty {
                MacReleaseFiles(
                    rows: tracks.files,
                    onOpenDocument: tracks.actions.sheetCaption.open,
                    sheetActions: tracks.actions.sheetCaption
                )
                .disabled(!editable)
            }
        }
    }
}

/// The release editor's action row, the one part its two hosts draw
/// differently.
enum ReleaseEditorActionRow {
    /// An import's: how many rows nobody named, where the import goes and
    /// Import, with the folder's check under the row; `nil` while the draft
    /// is blank and there is nothing to import.
    case importing(ImportCommitControls?)
    /// A form's: Cancel and Save, with the step of a catalog apply and why
    /// the form can't be saved under the row.
    case saving(ReleaseSaveRow)
}

/// A form's action row: Cancel and Save, the network read an apply waits
/// on while it runs, and the failure of the last save or check.
struct ReleaseSaveRow {
    let controls: ReleaseEditControls
    let applying: CatalogApplying?
    let notice: DisplayError?
}

/// A command in the release editor's Metadata menu, in both of its hosts.
/// Each replaces the values the person has, so the editor asks before
/// running it.
enum ReleaseMetadataCommand: CaseIterable, Identifiable {
    /// Replace the values with what the release's own files say.
    case resetToFileMetadata
    case clearMetadata

    var id: Self { self }

    var title: String {
        switch self {
        case .resetToFileMetadata: String(localized: "Reset to file metadata")
        case .clearMetadata: String(localized: "Clear metadata")
        }
    }

    /// What the confirmation asks.
    var question: String {
        switch self {
        case .resetToFileMetadata: String(localized: "Reset to file metadata?")
        case .clearMetadata: String(localized: "Clear metadata?")
        }
    }

    /// What the confirmation says the command changes and keeps.
    var consequence: String {
        switch self {
        case .resetToFileMetadata:
            String(
                localized:
                    "Replace metadata with what the files, sheets and folder say about the release. Tracks and audio assignments will remain unchanged."
            )
        case .clearMetadata:
            String(
                localized:
                    "The tracks and their audio files stay as they are."
            )
        }
    }

    /// Run the command through the host's action for it.
    func run(
        resetToFileMetadata: () -> Void,
        clearMetadata: () -> Void
    ) {
        switch self {
        case .resetToFileMetadata: resetToFileMetadata()
        case .clearMetadata: clearMetadata()
        }
    }
}

/// What the editor's own controls do, beside the fields: identify the
/// release, run the Metadata menu's commands, and unlink its records.
struct ReleaseEditorCommands {
    /// Identify the release from its files, showing the results.
    let identifyAutomatically: () -> Void
    /// Show the typed search, starting nothing.
    let search: () -> Void
    /// Run a command of the Metadata menu once the person confirms it.
    let metadataCommand: (ReleaseMetadataCommand) -> Void
    /// Drop the link and its records.
    let unlink: () -> Void
    /// Whether a read or a save in flight disables Automatic, Search and
    /// the menu.
    let identifyDisabled: Bool
}

/// What the card shows and where its edits go.
struct ReleaseEditorCard {
    /// The values the fields show. The card reads them in the header's own
    /// body, so a change to them redraws the header alone.
    let values: @MainActor () -> BridgeRawReleaseEdit
    /// The media and details the pressing's Add menus offer, as core's read
    /// hands them beside the values.
    let pressingOffers: BridgePressingOffers
    /// The album artists, each with its badge, as core's read found them;
    /// `nil` while the album names none.
    let albumArtists: BridgeArtistField?
    let writer: ReleaseFieldWriter
    let cover: ReleaseCover
    /// The audio's codec, rate, depth and channels; `nil` where nothing has
    /// read the files.
    let sourceAudio: BridgeSourceAudioSummary?
    /// The album the release files under; `nil` while the draft is blank
    /// and files under nothing yet.
    let album: BridgeDraftAlbum?
    let albumActions: DraftAlbumActions
    /// Every catalog record that describes the release, the applied
    /// catalog's first.
    let records: [BridgeReleaseRecord]
    /// Whether the link names an album and no pressing of it.
    let pressingUnknown: Bool
}

/// The card's cover and what it does: a click opens Change cover, and a
/// folder image dropped on it becomes the cover where the host takes one.
struct ReleaseCover {
    let content: ImageContent?
    /// Open Change cover; `nil` where there is no artwork to pick from.
    let onEdit: (() -> Void)?
    /// Send a folder image dropped on the cover, by its file id, for core
    /// to make the cover or refuse; `nil` where the cover takes no drop.
    let onDropImage: ((String) -> Void)?
}

/// What the editor shows under its card: the release's images, its track
/// table and its Files list, and what their controls do.
struct ReleaseEditorTracks {
    let images: [BridgeMappingImage]
    /// Open the images in the lightbox at the one whose file id it gets.
    let onOpenImages: (String) -> Void
    let sides: [BridgeTrackTableSide]
    let files: [BridgeMappingFileRow]
    /// The source window the preview player plays now, if any.
    let previewingTarget: BridgePreviewTarget?
    let actions: ReleaseTrackTableActions
}

/// The editor's card: the identify bar and the action row, their notices,
/// the header, and the records with Unlink.
private struct ReleaseEditorCardView: View {
    let actionRow: ReleaseEditorActionRow
    let commands: ReleaseEditorCommands
    let card: ReleaseEditorCard
    let editingCommands: EditingCommitCommands
    let editable: Bool

    @Environment(Library.self)
    private var library
    /// The Metadata menu's command the card is asking about.
    @State
    private var confirming: ReleaseMetadataCommand?

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            MacIdentifyBar(
                spacing: ThemeSpace.related,
                identifyDisabled: commands.identifyDisabled,
                onAutomatic: commands.identifyAutomatically,
                onSearch: commands.search,
                onMetadataCommand: { confirming = $0 },
                trailing: { trailing }
            )
            .disabled(!editable)
            notices
            CardHeader(
                card: card,
                editingCommands: editingCommands,
                artistSearch: library.artistSearch,
                canPickAlbum: editable
            )
            .disabled(!editable)
            // The catalogs that describe the release end the card, under
            // their own divider, beside the way to unlink them.
            if !card.records.isEmpty {
                MacHairline()
                records
            }
        }
        .padding(.vertical, ThemeSpace.edge)
        .padding(.horizontal, ThemeSpace.section)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
        .macAlert(item: $confirming) { command in
            (
                MacAlertWords(
                    Text(command.question),
                    message: Text(command.consequence)
                ),
                [
                    MacAlertButton(
                        Text(command.title),
                        role: .destructive
                    ) { commands.metadataCommand(command) },
                    MacAlertButton(Text("Cancel"), role: .cancel),
                ]
            )
        }
    }

    @ViewBuilder
    private var trailing: some View {
        switch actionRow {
        case .importing(let commit):
            if let commit {
                ImportCommitRow(commit: commit)
            }
        case .saving(let row):
            row.controls
        }
    }

    @ViewBuilder
    private var notices: some View {
        switch actionRow {
        case .importing(let commit):
            // What stops the import, in a notice under the row that offers
            // it.
            if let folderCheck = commit?.folderCheck?.localizedText {
                MacNotice(
                    title: folderCheck,
                    advice: nil,
                    error: nil,
                    tone: .warning
                )
            }
        case .saving(let row):
            if let applying = row.applying {
                CatalogApplyStepLine(apply: applying.apply)
            }
            if let notice = row.notice {
                MacNotice(error: notice)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var records: some View {
        HStack(alignment: .firstTextBaseline) {
            MacRecordLinks(records: card.records)
            if card.pressingUnknown {
                Text("Pressing unknown")
                    .themeText(.detail)
                    .foregroundStyle(Theme.macTextSecondary)
                    .fixedSize()
            }
            Spacer(minLength: ThemeSpace.group)
            MacButton("Unlink", kind: .borderless, action: commands.unlink)
                .help(
                    "Stop linking the release to these catalog records. The metadata stays as it is."
                )
                .disabled(!editable)
        }
    }
}

/// The editable header over the card's values. The header reads the values
/// in its own body, so a change to them redraws the header alone and not the
/// editor around it.
private struct CardHeader: View {
    let card: ReleaseEditorCard
    let editingCommands: EditingCommitCommands
    let artistSearch: ArtistSearchOpener
    let canPickAlbum: Bool

    var body: some View {
        ReleaseMetadataHeader(
            values: card.values(),
            pressingOffers: card.pressingOffers,
            albumArtists: card.albumArtists,
            writer: card.writer,
            editingCommands: editingCommands,
            artistSearch: artistSearch,
            cover: {
                ReleaseCoverWell(
                    coverContent: card.cover.content,
                    onEditCover: card.cover.onEdit,
                    onDropImage: card.cover.onDropImage
                )
            },
            audioFacts: {
                if let sourceAudio = card.sourceAudio {
                    SourceAudioLine(sourceAudio: sourceAudio)
                }
            },
            albumRow: {
                if let album = card.album {
                    MacAlbumPicker(
                        album: album,
                        canEdit: canPickAlbum,
                        actions: card.albumActions
                    )
                }
            }
        )
    }
}

/// The card's cover, or a plain tile where the release has none. A click
/// opens Change cover when there is artwork to pick from, and a folder image
/// dropped on it becomes the cover where the host takes one.
struct ReleaseCoverWell: View {
    let coverContent: ImageContent?
    /// Open Change cover; `nil` where there is no artwork to pick from.
    let onEditCover: (() -> Void)?
    /// Send a folder image dropped on the well, by its file id, for core to
    /// make the cover or refuse; `nil` where the well takes no drop.
    let onDropImage: ((String) -> Void)?

    static let coverSize = ReleaseMetadataLayout.coverSize

    @State
    private var dropTargeted = false

    var body: some View {
        Artwork(content: coverContent, pointSize: Self.coverSize)
            .frame(width: Self.coverSize, height: Self.coverSize)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            .contentShape(Rectangle())
            .onTapGesture {
                onEditCover?()
            }
            .accessibilityAddTraits(onEditCover == nil ? [] : .isButton)
            .accessibilityLabel(Text("Change cover"))
            .overlay {
                RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                    .stroke(
                        Theme.accent,
                        lineWidth: dropTargeted ? 3 : 0
                    )
            }
            .dropDestination(for: String.self) { fileIds, _ in
                guard let onDropImage, let fileId = fileIds.first else {
                    return false
                }
                onDropImage(fileId)
                return true
            } isTargeted: {
                dropTargeted = $0
            }
    }
}
