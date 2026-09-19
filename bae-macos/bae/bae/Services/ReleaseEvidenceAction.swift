import AppKit
import BaeKit
import SwiftUI

/// Opens the evidence requested by a chip in the window that owns it.
@MainActor
final class ReleaseEvidenceAction {
    private let read:
        @Sendable (BridgeEvidenceSubject, BridgeEvidenceSelection) async throws
            -> [BridgeEvidenceContent]
    private let uiStore: UiStore
    private var task: Task<Void, Never>?

    init(
        read:
            @escaping @Sendable (BridgeEvidenceSubject, BridgeEvidenceSelection)
            async throws -> [BridgeEvidenceContent],
        uiStore: UiStore
    ) {
        self.read = read
        self.uiStore = uiStore
    }

    func callAsFunction(
        _ subject: BridgeEvidenceSubject,
        _ selection: BridgeEvidenceSelection
    ) {
        task?.cancel()
        task = Task {
            do {
                let contents = try await read(subject, selection)
                try Task.checkCancellation()
                precondition(
                    !contents.isEmpty,
                    "Evidence read returned no files"
                )
                if contents.count == 1 {
                    present(contents[0])
                }
                else {
                    uiStore.presentModal {
                        EvidenceFilePicker(
                            contents: contents,
                            onSelect: self.present,
                            onClose: self.uiStore.dismissModal
                        )
                    }
                }
            }
            catch is CancellationError {
                // A newer chip request replaced this one.
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    private func present(_ content: BridgeEvidenceContent) {
        uiStore.dismissModal()
        switch content {
        case .document(let name, let text):
            uiStore.presentDocument(name: name, text: text)
        case .image(let name, let bytes):
            uiStore.presentLightbox(items: [
                LightboxItem(
                    id: name,
                    label: name,
                    previewContent: .bytes(Data(bytes))
                )
            ])
        case .reveal(let path):
            NSWorkspace.shared.activateFileViewerSelecting([
                URL(fileURLWithPath: path)
            ])
        }
    }
}

extension EnvironmentValues {
    @Entry
    var openReleaseEvidence: ReleaseEvidenceAction?
    @Entry
    var releaseEvidenceSubject: BridgeEvidenceSubject?
}

#if DEBUG
    extension View {
        /// Supplies the chip dependencies without reading the person's files.
        func releaseEvidencePreviewEnvironment(
            subject: BridgeEvidenceSubject
        ) -> some View {
            self
                .environment(\.releaseEvidenceSubject, subject)
                .environment(
                    \.openReleaseEvidence,
                    ReleaseEvidenceAction(
                        read: { _, _ in throw CancellationError() },
                        uiStore: UiStore()
                    )
                )
        }
    }
#endif
