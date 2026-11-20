import BaeKit
import SwiftUI

/// The one release editor: a library release's cover, fields, album and
/// tracks, with the catalog search that applies a release onto them, laid
/// out as the import pane lays out a folder's draft. The library's Edit…
/// sheet and the import pane of an imported folder both show it.
struct ReleaseEditorView: View {
    let session: ReleaseMetadataEditSession
    /// What the catalog search names the release by.
    let displayName: String
    var onPlayTrack: ((String) -> Void)?
    /// The images of the folder the release was imported from, as the scan
    /// read them, drawn between the album row and the tracks; `nil` where
    /// the release has no folder images to show.
    var images: [BridgeMappingImage]?
    var onOpenImages: ((String) -> Void)?
    /// The save landed; receives the album the release is on after it.
    let onSaved: @MainActor @Sendable (String) -> Void

    /// The draft, or the catalog search opened on one of its sections.
    @State
    private var searchSection: FindOnlineSection?
    /// The cover picker's lightbox image as it loads.
    @State
    private var coverLightboxLoad = LightboxImageLoad()

    @Environment(Importer.self)
    private var importer
    @Environment(ImportStore.self)
    private var importStore
    @Environment(ReleaseEditor.self)
    private var releaseEditor
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.openSettingsWindow)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation

    /// The key core runs the release's catalog search under.
    private var searchKey: Data {
        bridgeReidentifyKey(releaseId: session.releaseId)
    }

    var body: some View {
        Group {
            if let draft = session.draft {
                MacReleaseEditor(
                    actionRow: .saving(saveRow),
                    commands: ReleaseEditorCommands(
                        identifyAutomatically: identifyAutomatically,
                        search: { openSearch(.search) },
                        metadataCommand: { command in
                            command.run(
                                resetToFileMetadata: session.applyFileTags,
                                clearMetadata: session.clearMetadata
                            )
                        },
                        unlink: session.unlink,
                        identifyDisabled: session.isBusy
                    ),
                    card: card(draft),
                    tracks: ReleaseEditorTracks(
                        images: images,
                        onOpenImages: onOpenImages ?? { _ in },
                        sides: draft.sides,
                        files: draft.files?.map { .file(file: $0) },
                        previewingTarget: nil,
                        actions: tableActions
                    ),
                    editingCommands: session.editingCommands,
                    editable: !session.isBusy,
                    searching: searchSection != nil,
                    search: {
                        if let searchSection {
                            search(searchSection)
                        }
                    }
                )
            }
        }
        .onDisappear { endSearch() }
    }

    /// Cancel and Save, the step of an apply, and why the form can't be
    /// saved, each read where the row draws it.
    private var saveRow: ReleaseSaveRow {
        let session = session
        let onSaved = onSaved
        return ReleaseSaveRow(
            controls: {
                ReleaseEditControls(
                    changed: session.draft?.changed == true,
                    isBusy: session.isBusy,
                    reading: session.readingLine,
                    onCancel: {
                        searchSection = nil
                        session.cancelChanges()
                    },
                    onSave: { session.save(onSuccess: onSaved) }
                )
            },
            applying: { session.catalogApplying },
            notice: {
                session.validationMessage.map { DisplayError(line: $0) }
                    ?? session.failure
            }
        )
    }

    private func card(_ draft: BridgeReleaseEditDraft) -> ReleaseEditorCard {
        let session = session
        return ReleaseEditorCard(
            values: { session.values },
            pressingOffers: draft.pressingOffers,
            albumArtists: draft.albumArtists,
            writer: session.fieldWriter,
            cover: ReleaseCover(
                content: draft.cover.map { .libraryImage($0) },
                onEdit: presentCoverPicker,
                onDropImage: nil
            ),
            sourceAudio: draft.sourceAudio,
            album: draft.album,
            albumActions: session.albumActions,
            records: draft.records,
            pressingUnknown: false
        )
    }

    /// The table's actions on the session's form: a side or disc picked for
    /// a sheet, or an audio file for one of its lines. The caption offers
    /// neither *Ignored* nor binding no audio, since the editor keeps every
    /// track it opened on.
    private var tableActions: ReleaseTrackTableActions {
        let session = session
        return ReleaseTrackTableActions(
            sheetCaption: SheetCaptionActions(
                open: openDocument,
                place: { sheetId, place in
                    if case .on(let side) = place {
                        session.pickSheetSide(sheetId, side: side)
                    }
                },
                bind: { sheetId, fileReference, audioFileId in
                    if let audioFileId {
                        session.pickSheetFile(
                            sheetId,
                            fileReference: fileReference,
                            audioFileId: audioFileId
                        )
                    }
                }
            ),
            audition: TrackAuditionActions(
                preview: { _ in },
                stopPreview: {},
                playTrack: onPlayTrack
            ),
            editTrack: { await session.updateTrack($0) }
        )
    }

    /// The catalog search under this release's key; picking a pressing
    /// applies it to the session at once and goes back to the draft.
    @ViewBuilder
    private func search(_ section: FindOnlineSection) -> some View {
        if let candidate = importStore.reIdentifyCandidates[searchKey] {
            CandidateRuntimeReader(key: searchKey) { runtime in
                ImportSearchFlow.buildSearchPane(
                    services: ImportSearchFlow.ImportServices(
                        importer: importer,
                        importStore: importStore
                    ),
                    input: ImportSearchFlow.SearchPaneInput(
                        candidate: candidate,
                        key: searchKey,
                        runtime: runtime,
                        openSection: section,
                        onOpenSection: { searchSection = $0 },
                        // A library release has no folder draft to keep.
                        onKeepOwnDraft: nil,
                        // Nor a folder to link to an album.
                        onLinkSharedAlbum: nil,
                        // Its runs start straight away, never waiting on
                        // the identification queue.
                        onCancelIdentification: nil
                    ),
                    openSettings: {
                        settingsNavigation.open(
                            .importing,
                            present: { openSettings() }
                        )
                    },
                    onBack: { searchSection = nil },
                    onSelect: { pressing in
                        searchSection = nil
                        session.apply(pressing.link)
                    }
                )
            }
            .frame(maxWidth: .infinity)
            .card()
        }
    }

    /// Open Change cover over the editor; a pick writes the cover at once.
    private func presentCoverPicker() {
        let releaseEditor = releaseEditor
        let uiStore = uiStore
        let session = session
        let lightboxLoad = coverLightboxLoad
        uiStore.presentModal {
            CoverGalleryFrame {
                CoverSheetView(
                    owner: .release(releaseId: session.releaseId),
                    lightboxLoad: lightboxLoad,
                    onFindRelease: {
                        uiStore.dismissModal()
                        openSearch(.search)
                    },
                    refreshCovers: {
                        try await releaseEditor.refreshReleaseCovers(
                            session.releaseId
                        )
                    },
                    onSelect: { selection in
                        try await releaseEditor.changeCover(
                            session.releaseId,
                            selection
                        )
                    },
                    onDone: { uiStore.dismissModal() }
                )
            }
        }
    }

    /// Open the release's document `fileId` in the viewer.
    private func openDocument(name: String, fileId: String) {
        let read = releaseEditor.readReleaseDocument
        let releaseId = session.releaseId
        uiStore.openDocument(name: name) { try await read(releaseId, fileId) }
    }

    /// Open the search and start identifying the release from its files.
    private func identifyAutomatically() {
        openSearch(.automatic)
        let importer = importer
        let uiStore = uiStore
        let releaseId = session.releaseId
        Task {
            do { try await importer.autoIdentifyRelease(releaseId) }
            catch { uiStore.showError(error) }
        }
    }

    /// Open the search on `section`, starting nothing.
    private func openSearch(_ section: FindOnlineSection) {
        if importStore.reIdentifyCandidates[searchKey] == nil {
            importStore.reIdentifyCandidates[searchKey] = Candidate(
                reIdentifyKey: searchKey,
                releaseId: session.releaseId,
                displayName: displayName
            )
        }
        searchSection = section
    }

    /// End the release's identification in core and forget its search, so
    /// the editor opened again starts fresh.
    private func endSearch() {
        guard importStore.reIdentifyCandidates[searchKey] != nil else {
            return
        }
        importStore.reIdentifyCandidates.removeValue(forKey: searchKey)
        let importer = importer
        let uiStore = uiStore
        let releaseId = session.releaseId
        Task {
            do { try await importer.endReleaseIdentification(releaseId) }
            catch { uiStore.showError(error) }
        }
    }
}
