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
            openDocument: { name, path in openDocument(name: name, at: path) },
            openImages: { images, path in
                openGallery(images: images, at: path)
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

    private func openDocument(name: String, at path: String) {
        do {
            let text = try readTextFile(path: path)
            uiStore.presentDocument(name: name, text: text)
        }
        catch {
            // No line means a cancellation, which raises no alert.
            if let line = error.displayLine {
                uiStore.showError(
                    String(localized: "Could not read \(name): \(line)")
                )
            }
        }
    }

    /// Open the folder's images in the lightbox, starting at `path`.
    private func openGallery(images: [BridgeMappingImage], at path: String) {
        let items = images.map { image in
            LightboxItem(
                label: image.name,
                path: image.localPath
            )
        }
        guard !items.isEmpty else { return }
        uiStore.presentLightbox(items: items, preferring: path)
    }
}
