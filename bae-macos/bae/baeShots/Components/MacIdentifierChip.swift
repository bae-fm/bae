import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macIdentifierChipScenes: [ShotScene] = [
        identifierChipScene("mac.component.identifier-chip.off") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                style: .off,
                onPress: {}
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.on") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                onPress: {}
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.found") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                style: .found,
                onPress: {}
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.searching") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                onPress: {}
            ) {
                ChipTrailing(state: .searching, onRetry: {})
            }
        },
        identifierChipScene("mac.component.identifier-chip.failed") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                onPress: {},
                separatesTrailing: true
            ) {
                ChipTrailing(
                    state: .failed(failure: .timeout),
                    onRetry: {}
                )
            }
        },
        identifierChipScene("mac.component.identifier-chip.dash") {
            MacIdentifierChip(label: "Barcode")
        },
        identifierChipScene("mac.component.identifier-chip.title-search") {
            TitleSearchChip(
                album: "Low Tide Hours",
                artist: "Mara Quill",
                style: .on,
                onCommit: { _, _ in }
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.several-values") {
            MacIdentifierChip(label: "Barcode", values: severalValues) {
                EmptyView()
            }
        },
        component(
            "mac.component.identifier-chip.several-values-shown",
            width: 842,
            height: 119
        ) {
            ZStack(alignment: .topLeading) {
                MacIdentifierChip(label: "Barcode", values: severalValues) {
                    EmptyView()
                }
                moreValuesPopover(
                    severalCodes,
                    buttonCenter: severalValuesMoreButtonCenter,
                    buttonBottom: 15.5
                )
            }
            .frame(width: 842, height: 119, alignment: .topLeading)
        },
    ]

    private static let severalCodes = [
        "0 41234 56789 2", "0 41234 56789 3", "0 41234 56789 4",
    ]

    /// The codes as core hands them over: the first, and the count of two
    /// behind +N.
    private static let severalValues = MacIdentifierChipValues(
        shown: severalCodes[0],
        moreCount: 2,
        all: severalCodes
    )

    /// Where the +N button's centre is from the chip's left.
    private static let severalValuesMoreButtonCenter: CGFloat = 169.4

    private static func identifierChipScene<Chip: View>(
        _ id: String,
        _ chip: @escaping @MainActor () -> Chip
    ) -> ShotScene {
        component(
            id,
            width: 344,
            height: 40
        ) {
            chip()
        }
    }
}
