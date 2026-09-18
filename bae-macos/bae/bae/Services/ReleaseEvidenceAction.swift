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
                if contents.count == 1, case .reveal(let path) = contents[0] {
                    NSWorkspace.shared.activateFileViewerSelecting([
                        URL(fileURLWithPath: path)
                    ])
                    return
                }
                uiStore.presentModal {
                    EvidenceViewer(
                        contents: contents,
                        onClose: self.uiStore.dismissModal
                    )
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
}

extension EnvironmentValues {
    @Entry
    var openReleaseEvidence: ReleaseEvidenceAction?
    @Entry
    var releaseEvidenceSubject: BridgeEvidenceSubject?
}
