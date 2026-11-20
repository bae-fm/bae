import BaeKit
import SwiftUI

@testable import bae

// The import's draft card and the parts it is built from.

extension ShotScene {
    static let importDraftScenes: [ShotScene] =
        draftCardScenes + identifyBarScenes + releaseIdentityScenes
        + releaseFieldsScenes + albumPickerScenes + albumArtistFieldScenes
        + inlineFieldScenes

    // MARK: Values

    /// Low Tide Hours, a CD from Harbor Records, as the draft card holds it.
    private static func lowTideEdit() -> BridgeRawReleaseEdit {
        PreviewData.boardEdit(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            year: "1992",
            pressing: BridgeRawPressingEdit(
                year: "1992",
                labels: [
                    .alone(name: "Harbor Records", catalogNumber: "HR-1042")
                ],
                barcodes: [.blank],
                facts: PreviewData.pressingFacts(
                    country: "US",
                    media: PreviewData.media(.cd),
                    status: .official
                )
            ),
            tracks: []
        )
    }

    /// A draft that names nothing yet.
    private static func blankEdit() -> BridgeRawReleaseEdit {
        PreviewData.blankDraft(files: [])
    }

    /// The release Low Tide Hours by Mara Quill with the pressing `pressing`.
    private static func release(
        pressing: BridgeRawPressingEdit
    ) -> BridgeRawReleaseEdit {
        PreviewData.boardEdit(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            year: "1992",
            pressing: pressing,
            tracks: []
        )
    }

    private static let newArtistStanding = BridgeArtistFieldStanding(
        field: .new,
        artists: [.new]
    )

    /// The library album the records name.
    private static let libraryAlbum = BridgeDraftAlbum.named(
        albumId: "album-low-tide-hours",
        line: .titleAlone(title: "Low Tide Hours")
    )

    private static let inertAlbumActions = DraftAlbumActions(
        pick: { _ in },
        search: { _ in [] }
    )

    private static func artistSearch() -> ArtistSearchOpener {
        PreviewData.artistAssignmentsLibrary().artistSearch
    }

    /// The scan's reading of the folder's audio, which the audio line shows.
    private static func folderAudio() -> BridgeSourceAudioSummary? {
        draftSummary(blank: false).sourceAudio
    }

    /// What the draft card knows of the folder: its audio when `blank` is
    /// false, nothing when it is true.
    private static func draftSummary(blank: Bool) -> ImportReleaseSummary {
        let edit = blank ? blankEdit() : lowTideEdit()
        let candidate = PreviewData.paneCandidate(
            folder: PreviewData.boardFolder(
                .lowTideHours,
                files: PreviewData.boardFolderFiles(
                    tracks: blank ? [] : [PreviewData.northboundTracks[0]],
                    images: []
                )
            ),
            edit: edit,
            summaryCatalogNumber: blank ? nil : "HR-1042",
            mapping: BridgeMappingTable(
                images: [],
                trackSides: [],
                files: [],
                untitledTrackCount: 0
            )
        )
        return ImportReleaseSummary(candidate: candidate, editValues: edit)
    }

    private static func coverWell(_ cover: Bool) -> some View {
        ImportCoverWell(
            coverContent: cover
                ? ImageContent.localFile(
                    PreviewData.previewArtFile("Low Tide Hours")
                )
                : nil,
            hasCoverOptions: cover,
            onEditCover: {},
            onDropImage: nil
        )
    }

    /// The pop-up of `albumOptions` drawn open in place, its first line on
    /// the pop-up's line: `x` and `y` are where the pop-up's top left is,
    /// in the frame the menu is drawn over.
    private static func albumMenu(
        _ album: BridgeDraftAlbum,
        x: CGFloat,
        y: CGFloat
    ) -> some View {
        let menu = MacMenu(
            MacAlbumPicker.albumOptions(album),
            selection: MacAlbumPicker.Choice.standing,
            select: { _ in }
        )
        return MacMenuInPlace(
            { menu.entries },
            highlighted: menu.chosen,
            isPopUp: true
        )
        .offset(x: x - 31, y: y - 6)
    }

    // MARK: Draft card

    private static func commitControls(
        folderCheck: BridgeFolderCheck? = nil,
        importStatus: BridgeCandidateImportStatus? = nil
    ) -> ImportCommitControls {
        ImportCommitControls(
            untitledTrackCount: 0,
            folderCheck: folderCheck,
            importStatus: importStatus,
            importDestination: .constant(.cloud),
            actions: ImportCommitActions(
                confirmImport: {},
                viewInLibrary: { _ in }
            )
        )
    }

    /// The draft card over what the folder holds. `blank` leaves the draft
    /// naming nothing; `commit` is `nil` where there is nothing to import.
    private static func importDraft(
        blank: Bool = false,
        album: BridgeDraftAlbum? = .new,
        commit: ImportCommitControls? = commitControls(),
        syncs: Bool = false
    ) -> some View {
        let summary = draftSummary(blank: blank)
        return importStores(
            MacImportDraft(
                releaseSummary: summary,
                actionable: true,
                isReading: false,
                coverContent: blank
                    ? nil
                    : ImageContent.localFile(
                        PreviewData.previewArtFile("Low Tide Hours")
                    ),
                hasCoverOptions: !blank,
                editValues: blank ? blankEdit() : lowTideEdit(),
                pressingOffers: PreviewData.pressingOffers,
                albumArtistStanding: blank
                    ? nil : PreviewData.boardLibraryArtistStanding,
                records: blank ? [] : PreviewData.releaseRecordsPair,
                releaseLink: nil,
                album: album,
                albumActions: inertAlbumActions,
                editActions: ReleaseFieldWriter { _, _ in },
                editingCommands: EditingCommitCommands(),
                commit: commit,
                sourceActions: ImportReleaseSourceActions(
                    identifyAutomatically: {},
                    searchForRelease: {},
                    resetToFileMetadata: {},
                    clearMetadata: {},
                    unlink: {}
                ),
                onEditCover: {},
                onDropCoverImage: { _ in }
            )
            .frame(width: 720, alignment: .topLeading),
            syncs: syncs
        )
    }

    private static func draftScene<Draft: View>(
        _ id: String,
        height: CGFloat = 540,
        _ draft: @escaping @MainActor () -> Draft
    ) -> ShotScene {
        component(
            id,
            width: 736,
            height: height,
            images: PreviewData.artImageStore()
        ) {
            draft()
        }
    }

    private static let draftCardScenes: [ShotScene] = [
        draftScene("mac.component.import-draft.filled") { importDraft() },
        draftScene("mac.component.import-draft.import-to-the-cloud") {
            importDraft(syncs: true)
        },
        draftScene("mac.component.import-draft.folder-check", height: 590) {
            importDraft(
                commit: commitControls(folderCheck: .sourceTracksUnknown)
            )
        },
        draftScene("mac.component.import-draft.retry") {
            importDraft(
                commit: commitControls(
                    importStatus: .error(
                        error: .Diagnostic(
                            category: .import,
                            detail: "decode verification failed for 1 track"
                        )
                    )
                )
            )
        },
        draftScene("mac.component.import-draft.nothing-to-import") {
            importDraft(commit: nil)
        },
        draftScene("mac.component.import-draft.album-picker-open") {
            ZStack(alignment: .topLeading) {
                importDraft(album: libraryAlbum)
                albumMenu(libraryAlbum, x: 209, y: 160)
            }
        },
        draftScene("mac.component.import-draft.empty", height: 480) {
            importDraft(blank: true)
        },
    ]

    // MARK: Identify bar

    private static func identifyBarScene(
        _ id: String,
        syncs: Bool = false,
        commit: ImportCommitControls? = nil,
        editor: Bool? = nil
    ) -> ShotScene {
        component(
            id,
            width: 656,
            height: 50
        ) {
            importStores(
                identifyBar(commit: commit, editor: editor)
                    .frame(width: 640, alignment: .leading),
                syncs: syncs
            )
        }
    }

    /// The bar over an import's `commit` controls, or over a library
    /// release's Save and Cancel, `changed` once something is edited.
    @ViewBuilder
    private static func identifyBar(
        commit: ImportCommitControls?,
        editor changed: Bool?
    ) -> some View {
        if let changed {
            MacIdentifyBar(
                spacing: ThemeSpace.edge,
                identifyDisabled: false,
                onAutomatic: {},
                onSearch: {},
                onMetadataCommand: nil,
                trailing: {
                    ReleaseEditControls(
                        changed: changed,
                        isBusy: false,
                        onCancel: {},
                        onSave: {}
                    )
                }
            )
        }
        else {
            MacIdentifyBar(
                spacing: ThemeSpace.related,
                identifyDisabled: false,
                onAutomatic: {},
                onSearch: {},
                onMetadataCommand: { _ in },
                trailing: {
                    if let commit {
                        ImportCommitRow(commit: commit)
                    }
                }
            )
        }
    }

    private static let identifyBarScenes: [ShotScene] = [
        identifyBarScene(
            "mac.component.identify-bar.import",
            commit: commitControls()
        ),
        identifyBarScene(
            "mac.component.identify-bar.import-to-the-cloud",
            syncs: true,
            commit: commitControls()
        ),
        identifyBarScene(
            "mac.component.identify-bar.retry",
            commit: commitControls(
                importStatus: .error(
                    error: .Diagnostic(
                        category: .import,
                        detail: "decode verification failed for 1 track"
                    )
                )
            )
        ),
        identifyBarScene("mac.component.identify-bar.nothing-to-import"),
        identifyBarScene("mac.component.identify-bar.editor", editor: false),
        identifyBarScene(
            "mac.component.identify-bar.editor-changed",
            editor: true
        ),
    ]

    // MARK: Release identity

    private static func identity<Cover: View>(
        _ values: BridgeRawReleaseEdit,
        standing: BridgeArtistFieldStanding?,
        album: BridgeDraftAlbum?,
        audio: Bool,
        @ViewBuilder cover: @escaping () -> Cover
    ) -> some View {
        let sourceAudio = audio ? folderAudio() : nil
        return importStores(
            MacReleaseIdentity(
                values: values,
                albumArtistStanding: standing,
                writer: ReleaseFieldWriter { _, _ in },
                editingCommands: EditingCommitCommands(),
                artistSearch: artistSearch(),
                cover: cover,
                audioFacts: {
                    if let sourceAudio {
                        SourceAudioLine(sourceAudio: sourceAudio)
                    }
                },
                albumRow: {
                    if let album {
                        MacAlbumPicker(
                            album: album,
                            canEdit: true,
                            actions: inertAlbumActions
                        )
                    }
                }
            )
            .frame(width: 560, alignment: .leading)
        )
    }

    private static func identityScene<Identity: View>(
        _ id: String,
        height: CGFloat = 160,
        _ identity: @escaping @MainActor () -> Identity
    ) -> ShotScene {
        component(
            id,
            width: 576,
            height: height,
            images: PreviewData.artImageStore()
        ) {
            identity()
        }
    }

    private static let releaseIdentityScenes: [ShotScene] = [
        identityScene("mac.component.release-identity.filled") {
            identity(
                lowTideEdit(),
                standing: PreviewData.boardLibraryArtistStanding,
                album: libraryAlbum,
                audio: true
            ) { coverWell(true) }
        },
        identityScene("mac.component.release-identity.new-artist") {
            identity(
                lowTideEdit(),
                standing: newArtistStanding,
                album: .new,
                audio: true
            ) { coverWell(true) }
        },
        identityScene("mac.component.release-identity.no-cover") {
            identity(
                lowTideEdit(),
                standing: PreviewData.boardLibraryArtistStanding,
                album: nil,
                audio: true
            ) { coverWell(false) }
        },
        identityScene("mac.component.release-identity.long-title") {
            identity(
                PreviewData.boardEdit(
                    title:
                        "Low Tide Hours and Other Songs from the Harbor at Night",
                    artist: "Mara Quill",
                    year: "1992",
                    tracks: []
                ),
                standing: PreviewData.boardLibraryArtistStanding,
                album: .new,
                audio: true
            ) { coverWell(true) }
        },
        identityScene("mac.component.release-identity.nothing-under-it") {
            identity(
                lowTideEdit(),
                standing: PreviewData.boardLibraryArtistStanding,
                album: nil,
                audio: false
            ) { coverWell(true) }
        },
        identityScene("mac.component.release-identity.empty") {
            identity(
                blankEdit(),
                standing: nil,
                album: nil,
                audio: false
            ) { coverWell(false) }
        },
    ]

    // MARK: Release fields

    private static func pressing(
        label: String = "Harbor Records",
        catalog: String = "HR-1042",
        barcode: String? = nil,
        packaging: BridgePackaging? = nil,
        details: [BridgeDiscogsDetail] = []
    ) -> BridgeRawPressingEdit {
        BridgeRawPressingEdit(
            year: "1992",
            labels: [
                .alone(name: label, catalogNumber: catalog)
            ],
            barcodes: barcode.map { [.alone($0)] } ?? [.blank],
            facts: PreviewData.pressingFacts(
                country: "US",
                media: PreviewData.media(.cd),
                status: .official,
                packaging: packaging,
                discogsDetails: details
            )
        )
    }

    private static func releaseFieldsScene(
        _ id: String,
        _ values: BridgeRawReleaseEdit
    ) -> ShotScene {
        component(
            id,
            width: 576,
            height: 300
        ) {
            importStores(
                MacReleaseFields(
                    values: values,
                    pressingOffers: PreviewData.pressingOffers,
                    writer: ReleaseFieldWriter { _, _ in },
                    editingCommands: EditingCommitCommands()
                )
                .frame(width: 560, alignment: .leading)
            )
        }
    }

    private static let releaseFieldsScenes: [ShotScene] = [
        releaseFieldsScene(
            "mac.component.release-fields.filled",
            release(
                pressing: pressing(
                    barcode: "0 41234 56789 2",
                    packaging: .jewelCase,
                    details: [.remastered]
                )
            )
        ),
        releaseFieldsScene(
            "mac.component.release-fields.several-details",
            release(
                pressing: pressing(
                    barcode: "0 41234 56789 2",
                    packaging: .jewelCase,
                    details: [.remastered, .limitedEdition]
                )
            )
        ),
        releaseFieldsScene(
            "mac.component.release-fields.partly-filled",
            release(pressing: pressing())
        ),
        releaseFieldsScene(
            "mac.component.release-fields.long-values",
            release(
                pressing: pressing(
                    label: "Harbor Records and the Northern Sound Company",
                    catalog: "HR-1042-2-REISSUE-DELUXE",
                    barcode: "0 41234 56789 2",
                    packaging: .jewelCase,
                    details: [.limitedEdition, .reissue, .remastered]
                )
            )
        ),
        releaseFieldsScene("mac.component.release-fields.empty", blankEdit()),
    ]

    // MARK: Album picker

    private static func albumPickerScene(
        _ id: String,
        _ album: BridgeDraftAlbum,
        open: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 400,
            height: open ? 146 : 40
        ) {
            ZStack(alignment: .topLeading) {
                MacAlbumPicker(
                    album: album,
                    canEdit: true,
                    actions: inertAlbumActions
                )
                .frame(width: 380, alignment: .leading)
                if open {
                    albumMenu(album, x: 41, y: 0)
                }
            }
        }
    }

    private static let albumPickerScenes: [ShotScene] = [
        albumPickerScene("mac.component.album-picker.new-album", .new),
        albumPickerScene(
            "mac.component.album-picker.library-album",
            libraryAlbum
        ),
    ]

    // MARK: Album artist field

    private static func albumArtistFieldScene(
        _ id: String,
        artist: Bool,
        standing: BridgeArtistFieldStanding?
    ) -> ShotScene {
        component(
            id,
            width: 300,
            height: 40
        ) {
            importStores(
                MacAlbumArtistField(
                    assignments: artist
                        ? [PreviewData.boardArtist("Mara Quill")] : [],
                    standing: standing,
                    placeholder: "Album artist",
                    artistSearch: artistSearch(),
                    onChange: { _ in }
                )
                .frame(width: 284, alignment: .leading)
            )
        }
    }

    private static let albumArtistFieldScenes: [ShotScene] = [
        albumArtistFieldScene(
            "mac.component.album-artist-field.library-artist",
            artist: true,
            standing: PreviewData.boardLibraryArtistStanding
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.new-artist",
            artist: true,
            standing: newArtistStanding
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.no-chip",
            artist: true,
            standing: nil
        ),
        albumArtistFieldScene(
            "mac.component.album-artist-field.no-artist",
            artist: false,
            standing: nil
        ),
    ]

    // MARK: Inline field

    /// One `MacInlineField` in the preview's 260pt column; `large` is the
    /// album title's size and `quiet` the album year's tertiary colour.
    private static func inlineFieldScene(
        _ id: String,
        placeholder: String,
        value: String,
        large: Bool = false,
        quiet: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 276,
            height: large ? 46 : 38
        ) {
            MacInlineField(
                placeholder: placeholder,
                value: value,
                name: placeholder,
                fillsWidth: true,
                font: large ? ThemeText.title.nsFont : ThemeText.body.nsFont,
                textColor: quiet
                    ? .tertiaryLabelColor : NSColor(Theme.textPrimary),
                placeholderRole: large || quiet ? .hint : .emptyMark,
                editingCommands: EditingCommitCommands(),
                onCommit: { _ in }
            )
            .frame(width: 260, alignment: .leading)
        }
    }

    private static let inlineFieldScenes: [ShotScene] = [
        inlineFieldScene(
            "mac.component.inline-field.value",
            placeholder: "—",
            value: "Harbor Records"
        ),
        inlineFieldScene(
            "mac.component.inline-field.empty",
            placeholder: "—",
            value: ""
        ),
        inlineFieldScene(
            "mac.component.inline-field.long-value",
            placeholder: "—",
            value: "Harbor Records and the Northern Sound Company"
        ),
        inlineFieldScene(
            "mac.component.inline-field.large",
            placeholder: "Album title",
            value: "Low Tide Hours",
            large: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.large-empty",
            placeholder: "Album title",
            value: "",
            large: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.quiet",
            placeholder: "Album year",
            value: "1992",
            quiet: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.quiet-empty",
            placeholder: "Album year",
            value: "",
            quiet: true
        ),
    ]
}
