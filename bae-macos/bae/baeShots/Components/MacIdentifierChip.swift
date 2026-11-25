import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macIdentifierChipScenes: [ShotScene] = [
        identifierChipScene("mac.component.identifier-chip.on") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                onPress: {}
            ) {
                EmptyView()
            }
        },
        identifierChipScene("mac.component.identifier-chip.off") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                style: .outlined,
                onPress: {}
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.dash") {
            MacIdentifierChip(label: "Barcode")
        },
        identifierChipScene("mac.component.identifier-chip.found") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                onPress: {}
            ) {
                capsule(.musicBrainz, .found(count: 2, groups: []))
                capsule(.discogs, .noMatch)
            }
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
                    buttonBottom: 16
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
    private static let severalValuesMoreButtonCenter: CGFloat = 160.3

    private static func capsule(
        _ source: BridgeLookupCatalog,
        _ lookup: BridgeLookupState
    ) -> some View {
        ProviderCapsule(source: source, lookup: lookup, onRetry: {})
    }

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
