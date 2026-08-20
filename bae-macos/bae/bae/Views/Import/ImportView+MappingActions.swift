import BaeKit
import SwiftUI

// MARK: - What the mapping pane calls back into

extension ImportView {
    /// The services the pane's controls drive, with errors landing on the
    /// app's alert and documents and images landing on this view's overlays.
    var mappingServices: ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: importStore,
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

    /// Switch what the folder is read as. Unknown reads its own file tags;
    /// Release re-picks the release the candidate already holds, and opens the
    /// search when it holds none — there is nothing to go back to then.
    func setIdentity(_ identity: ImportIdentity, for candidate: Candidate) {
        switch (identity, candidate.pick) {
        case (.unknown, _):
            ImportSearchFlow.decideIdentity(
                importer: importer,
                importStore: importStore,
                key: candidate.key,
                pick: .unknown
            )
        case (.release, .some(let pick)):
            ImportSearchFlow.decideIdentity(
                importer: importer,
                importStore: importStore,
                key: candidate.key,
                pick: .release(
                    source: pick.source,
                    releaseId: pick.releaseId,
                    claim: pick.claim
                )
            )
        case (.release, .none):
            presentSearch(for: candidate)
        }
    }

    private func openDocument(name: String, at path: String) {
        do {
            let text = try readTextFile(path: path)
            documentContent = (name: name, text: text)
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
                id: image.localPath,
                label: image.name,
                source: .local(path: image.localPath)
            )
        }
        guard !items.isEmpty else { return }
        uiStore.presentLightbox(items: items, preferring: path)
    }
}
