import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macIdentifierChipScenes: [ShotScene] = [
        identifierChipScene("mac.component.identifier-chip.filled") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                onPress: {}
            ) {
                EmptyView()
            }
        },
        identifierChipScene("mac.component.identifier-chip.outlined") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                style: .outlined,
                onPress: {}
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.struck") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                style: .struck,
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
        identifierChipScene("mac.component.identifier-chip.searching") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                onPress: {}
            ) {
                capsule(.musicBrainz, .found(count: 2, groups: []))
                capsule(.discogs, .lookingUp)
            }
        },
        identifierChipScene("mac.component.identifier-chip.editable") {
            MacIdentifierChip(label: "Barcode") {
                Button {
                } label: {
                    Text(sampleBarcode)
                        .themeText(.mono)
                        .foregroundStyle(Theme.textPrimary)
                }
                .buttonStyle(.accentPlain)
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
                    severalValues,
                    buttonCenter: severalValuesMoreButtonCenter,
                    buttonBottom: 16
                )
            }
            .frame(width: 842, height: 119, alignment: .topLeading)
        },
    ]

    private static let severalValues = [
        "0 41234 56789 2", "0 41234 56789 3", "0 41234 56789 4",
    ]

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
