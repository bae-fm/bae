import BaeKit
import SwiftUI

// MARK: - What the mapping pane calls back into

extension ImportView {
    /// The services the pane's controls drive, with errors landing on the
    /// app's alert and documents and images landing on the window's overlays.
    var mappingServices: ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: importStore,
            endEditing: commitAndEndEditing,
            previewAudio: previewAudio,
            openDocument: { key, name, fileId in
                openDocument(name: name, fileId: fileId, of: key)
            },
            openImages: { key, fileId in
                openGallery(of: key, at: fileId)
            },
            onError: { uiStore.showError($0) },
        )
    }

    func mappingActions(for candidate: Candidate) -> ImportMappingActions {
        ImportMappingFlow.actions(
            key: candidate.key,
            services: mappingServices
        )
    }

    /// Show identification's results for this candidate, as core decides.
    func identify(_ candidate: Candidate) {
        ImportMappingFlow.identify(candidate, services: mappingServices)
    }

    /// Open the Find online page on its typed search, starting nothing.
    func searchForRelease(_ candidate: Candidate) {
        movePane(.search, for: candidate)
    }

    /// Move the pane as the person asked.
    func movePane(_ paneMove: BridgePaneMove, for candidate: Candidate) {
        ImportMappingFlow.movePane(
            paneMove,
            for: candidate,
            services: mappingServices
        )
    }

    private func openDocument(name: String, fileId: String, of key: Data) {
        let importer = importer
        uiStore.openDocument(name: name) {
            try await importer.readCandidateDocument(key, fileId)
        }
    }

    /// Open the folder images of the candidate at `key` in the lightbox,
    /// starting at the image whose file id is `fileId`.
    private func openGallery(of key: Data, at fileId: String) {
        guard let query = importer.folderImages() else { return }
        let uiStore = uiStore
        uiStore.presentLightbox(
            FolderImagesLightbox(
                candidateKey: key,
                focusedImage: fileId,
                query: query,
                onError: { uiStore.showError($0) }
            )
        )
    }
}
