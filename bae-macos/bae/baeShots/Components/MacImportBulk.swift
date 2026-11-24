import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportBulk`, one shot for each row of its preview.
    static let macImportBulkScenes: [ShotScene] =
        [
            component(
                "mac.component.import-bulk.several",
                width: 842,
                height: 568
            ) {
                importBulk(height: 568)
            },
            component(
                "mac.component.import-bulk.to-the-cloud",
                width: 842,
                height: 568
            ) {
                importBulk(syncs: true, height: 568)
            },
        ] + importBulkRunningScenes

    /// `MacImportBulk` while its Identify action runs, ready once the
    /// selection holds the run.
    private static let importBulkRunningScenes: [ShotScene] = {
        let running = RunningBulk()
        return [
            component(
                "mac.component.import-bulk.running",
                width: 842,
                height: 601,
                isReady: { running.selection?.running != nil }
            ) {
                let selection = runningSelection()
                running.selection = selection
                return importBulk(selection: selection, height: 601)
            }
        ]
    }()

    /// A selection whose Identify action is under way, its bar at two
    /// folders of five and never finishing.
    @MainActor
    private static func runningSelection() -> ImportSelection {
        let selection = ImportSelection.onList(
            operations: .stub(
                progressStream: {
                    .bulkActionProgress(
                        holding: BridgeSelectionActionProgress(
                            completed: 2,
                            total: 5,
                            fraction: 0.4
                        )
                    )
                },
                run: { _, _, _, _ in try await Task.neverAnswers() }
            )
        )
        Task { await selection.perform(.identify, uiStore: UiStore()) }
        return selection
    }

    @MainActor
    private static func importBulk(
        syncs: Bool = false,
        selection: ImportSelection = ImportSelection(),
        height: CGFloat = 600
    ) -> some View {
        selection.apply(PreviewData.fiveFolderSelectionSummary)
        return importStores(
            MacImportBulk(
                importDestination: .constant(.cloud),
                onPerform: { _ in }
            )
            .environment(selection)
            .frame(width: 842, height: height),
            syncs: syncs
        )
    }

    /// The selection of the running scene, which it keeps for `isReady`.
    @MainActor
    private final class RunningBulk {
        var selection: ImportSelection?
    }
}
