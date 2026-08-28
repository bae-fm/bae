import BaeKit
import SwiftUI

// MARK: - What the mapping pane calls back into

extension ImportView {
    /// The services the pane's controls drive, with errors landing on the
    /// app's alert and documents and images landing on this view's overlays.
    var mappingServices: ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            automaticIdentification: configStore.config
                .automaticImportIdentification,
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

    /// Put the draft or one source browser in the metadata slot.
    func presentMetadata(
        _ presentation: CandidateMetadataPresentation,
        for candidate: Candidate
    ) {
        ImportMappingFlow.presentMetadata(
            presentation,
            for: candidate,
            services: mappingServices
        )
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
