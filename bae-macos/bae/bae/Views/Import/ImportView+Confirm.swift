import BaeKit
import SwiftUI

/// Commit tail for a confirmed import. Shapes the candidate's raw editor
/// form into the wire edit — writing bae-core's `.invalid` reason onto the
/// candidate and bailing if it doesn't validate (the Import button is
/// disabled while invalid, so that path is defensive) — then runs `start`,
/// writing any thrown error onto the candidate.
@MainActor
private func commitImport(
    store: ImportStore,
    key: String,
    rawEdit: BridgeRawReleaseEdit,
    start: (BridgeReleaseUserEdit) throws -> Void
) {
    let userEdit: BridgeReleaseUserEdit
    switch shapeReleaseEdit(raw: rawEdit) {
    case .valid(let edit):
        userEdit = edit
    case .invalid(let reason):
        store.mutateCandidate(forKey: key) {
            $0.error = reason.localizedMessage
        }
        return
    }
    do {
        try start(userEdit)
    }
    catch {
        store.mutateCandidate(forKey: key) {
            $0.error = error.displayLine
        }
    }
}

// MARK: - Search, results, and confirm

extension ImportView {
    func searchAndResultsPane(
        for candidate: Candidate,
        selectedReleaseId: String?
    ) -> some View {
        ImportSearchFlow.buildSearchPane(
            services: ImportSearchFlow.ImportServices(
                importer: importer,
                library: library,
                importStore: importStore,
                configStore: configStore
            ),
            input: ImportSearchFlow.SearchPaneInput(
                candidate: candidate,
                key: candidate.key,
                localTrackCount: candidate.trackCount,
                selectedReleaseId: selectedReleaseId
            ),
            openSettings: { openSettings() },
            onAddAsUnknown: {
                guard case .folder(let folderPath, _) = candidate.source else {
                    return
                }
                ImportSearchFlow.addAsUnknown(
                    importer: importer,
                    importStore: importStore,
                    key: candidate.key,
                    folderPath: folderPath
                )
            },
        )
    }

    func confirmationView(for candidate: Candidate) -> some View {
        let key = candidate.key
        let detail = candidate.releaseDetailBridge
        // For Unknown imports there's no source release detail —
        // remote cover art and library status are absent, the track
        // count comes from the editor (one entry per audio file), and
        // there's nothing to mismatch against.
        let remoteCoverArts = detail?.coverArt ?? []
        let hasCoverOptions =
            !remoteCoverArts.isEmpty
            || (candidate.files.artwork.isEmpty == false)
        let libraryStatus =
            detail.flatMap { candidate.libraryStatuses[$0.releaseId] }
        let trackCountMismatch = detail?.trackCountMismatch ?? false
        let expectedTrackCount: UInt32 = {
            if let detailCount = detail?.trackCount {
                return detailCount
            }
            if let editTracks = candidate.editValues?.tracks {
                return UInt32(editTracks.count)
            }
            return 0
        }()
        return ImportSearchFlow.buildConfirmationView(
            inputs: ImportSearchFlow.ConfirmationInputs(
                importStore: importStore,
                key: key,
                uiStore: uiStore,
                trackCountMismatch: trackCountMismatch,
                expectedTrackCount: expectedTrackCount,
                libraryStatus: libraryStatus,
                remoteCoverArts: remoteCoverArts,
                hasCoverOptions: hasCoverOptions,
                storageManaged: $storageManaged,
                storagePinned: $storagePinned,
                localArtwork: candidate.files.artwork
            ),
            callbacks: ImportSearchFlow.ConfirmationCallbacks(
                onConfirmImport: {
                    commitConfirmedImport(candidate: candidate)
                },
                onViewInLibrary: { albumId in
                    uiStore.navigateToAlbum(albumId)
                }
            ),
            coverContent: {
                folderCoverThumb(
                    source: candidate.selectedCover.map {
                        ImageLoader.Source(bridge: $0.thumbnailSource)
                    },
                )
            },
        )
    }

    // MARK: - Folder-specific cover thumbnail (supports local artwork)

    private func folderCoverThumb(source: ImageLoader.Source?)
        -> some View
    {
        Group {
            if let source {
                ImageView(
                    source: source,
                    pointSize: 80
                )
            }
            else {
                Theme.placeholder
            }
        }
        .frame(width: 80, height: 80)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private func commitConfirmedImport(candidate: Candidate) {
        guard case .folder(let folderPath, _) = candidate.source else {
            return
        }
        // Start each attempt from a clean error state so a prior failed
        // commit's banner doesn't linger over a now-succeeding retry.
        importStore.mutateCandidate(forKey: candidate.key) { $0.error = nil }
        let coverSelection = candidate.selectedCover?.selection

        let storageMode = configStore.config.importStorageMode(
            managed: storageManaged
        )

        // The identity choice was picked at row-time (or set to
        // `.unknown` by the "Add as Unknown" link) and stashed on the
        // candidate; the editor overlay is the candidate's current
        // `editValues` (seeded from the detail or file-tag projection,
        // possibly mutated by the user on this confirmation page).
        // Both fields are written before the candidate transitions to
        // `.confirming` mode, which is the only mode that surfaces
        // this commit button — absence here is a structural bug.
        guard let identityChoice = candidate.identityChoice,
            let editValues = candidate.editValues
        else {
            fatalError("commit reached without identity choice or edit values")
        }

        commitImport(
            store: importStore,
            key: candidate.key,
            rawEdit: editValues
        ) {
            try importer.startImport(
                candidate.key,
                folderPath,
                coverSelection,
                storageMode,
                storagePinned,
                identityChoice,
                $0
            )
        }
    }
}
