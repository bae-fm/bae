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
    ]

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
