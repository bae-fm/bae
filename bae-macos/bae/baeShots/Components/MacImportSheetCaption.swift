import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportSheetCaption`, one shot for each row of its preview.
    static let macImportSheetCaptionScenes: [ShotScene] = [
        component(
            "mac.component.import-sheet-caption.default",
            page: true,
            width: 580,
            height: 110
        ) {
            VStack(alignment: .leading, spacing: 0) {
                ReleaseSheetCaption(
                    sheet: PreviewData.fieldRecordingsSheet(
                        1,
                        bound: .describes(
                            container: PreviewData.fieldRecordingsContainer
                        )
                    ),
                    actions: inertMappingActions().sheetCaption
                )
                ReleaseSheetCaption(
                    sheet: PreviewData.fieldRecordingsSheet(
                        2,
                        bound: .unresolved(
                            requested: ["Field Recordings CD2.flac"]
                        )
                    ),
                    actions: inertMappingActions().sheetCaption
                )
            }
            .frame(width: 580, alignment: .leading)
        }
    ]
}
