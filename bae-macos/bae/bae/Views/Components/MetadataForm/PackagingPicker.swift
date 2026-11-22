import BaeKit
import SwiftUI

/// What the release is sold in.
struct PackagingPicker: View {
    let packaging: BridgePackaging?
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                MacMenuItem("Not stated", checked: packaging == nil) {
                    choose(nil)
                }
                for option in bridgePackagings() {
                    MacMenuItem(
                        verbatim: Self.text(option),
                        checked: packaging == option
                    ) { choose(option) }
                }
            }
        ) {
            FactMenuLabel(text: packaging.map(Self.text))
        }
        .fixedSize()
        .accessibilityValue(
            Text(verbatim: FactMenuLabel.shown(packaging.map(Self.text)))
        )
    }

    static func text(_ packaging: BridgePackaging) -> String {
        localizedCoreString(bridgePackagingKey(packaging: packaging))
    }

    private func choose(_ packaging: BridgePackaging?) {
        Task { await write(.packaging(packaging: packaging)) }
    }
}
