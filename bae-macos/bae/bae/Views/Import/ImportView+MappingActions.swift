import BaeKit
import SwiftUI

// MARK: - What the mapping pane calls back into

extension ImportView {
    /// The services the pane's controls drive, with errors landing on the
    /// app's alert.
    var mappingServices: ImportMappingServices {
        ImportMappingServices(
            importer: importer,
            importStore: importStore,
            previewAudio: previewAudio,
            onError: { uiStore.showError($0) },
        )
    }

    func roleActions(for candidate: Candidate) -> ImportRoleActions {
        let key = candidate.key
        let services = mappingServices
        return ImportRoleActions(
            setRole: { fileId, choice in
                Task { @MainActor in
                    await ImportMappingFlow.setRole(
                        key: key,
                        fileId: fileId,
                        choice: choice,
                        services: services
                    )
                }
            },
            bindSheet: { sheetFileId, audioFileId in
                Task { @MainActor in
                    await bindSheet(key, sheetFileId, to: audioFileId)
                }
            },
            openDocument: { file in openDocument(file) },
            openImage: { path in openGallery(candidate, at: path) },
        )
    }

    func slotActions(for candidate: Candidate) -> ImportSlotActions {
        ImportMappingFlow.slotActions(
            key: candidate.key,
            services: mappingServices
        )
    }

    private func openDocument(_ file: BridgeFileInfo) {
        do {
            let text = try readTextFile(path: file.localPath)
            documentContent = (name: file.name, text: text)
        }
        catch {
            uiStore.showError(
                String(
                    localized:
                        "Could not read \(file.name): \(error.displayLine)"
                )
            )
        }
    }

    /// Open the folder's images in the lightbox, starting at `path`.
    private func openGallery(_ candidate: Candidate, at path: String) {
        let items = candidate.files.images.map { file in
            LightboxItem(
                id: file.file.localPath,
                label: file.file.name,
                source: .local(path: file.file.localPath)
            )
        }
        guard !items.isEmpty else { return }
        uiStore.presentLightbox(items: items, preferring: path)
    }
}
