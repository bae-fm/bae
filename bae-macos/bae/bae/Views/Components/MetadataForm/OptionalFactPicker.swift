import BaeKit
import SwiftUI

/// A fact of the pressing chosen from a menu of the vocabulary's values, or
/// left not stated: the release's status, or what it is sold in.
struct OptionalFactPicker<Value: Equatable>: View {
    let value: Value?
    /// The vocabulary's values, in the order the menu lists them.
    let options: [Value]
    let text: (Value) -> String
    /// The edit that sets the fact to a value, or to not stated.
    let edit: (Value?) -> BridgePressingFactEdit
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                MacMenuItem("Not stated", checked: value == nil) {
                    choose(nil)
                }
                for option in options {
                    MacMenuItem(
                        verbatim: text(option),
                        checked: value == option
                    ) { choose(option) }
                }
            }
        ) {
            FactMenuLabel(text: value.map(text))
        }
        .fixedSize()
        .accessibilityValue(
            Text(verbatim: FactMenuLabel.shown(value.map(text)))
        )
    }

    private func choose(_ value: Value?) {
        let edit = edit(value)
        Task { await write(edit) }
    }
}

extension OptionalFactPicker where Value == BridgeReleaseStatus {
    /// How official the release is.
    static func status(
        _ status: BridgeReleaseStatus?,
        write: @escaping @MainActor (BridgePressingFactEdit) async -> Void
    ) -> Self {
        Self(
            value: status,
            options: bridgeReleaseStatuses(),
            text: { localizedCoreString(bridgeReleaseStatusKey(status: $0)) },
            edit: { .status(status: $0) },
            write: write
        )
    }
}

extension OptionalFactPicker where Value == BridgePackaging {
    /// What the release is sold in.
    static func packaging(
        _ packaging: BridgePackaging?,
        write: @escaping @MainActor (BridgePressingFactEdit) async -> Void
    ) -> Self {
        Self(
            value: packaging,
            options: bridgePackagings(),
            text: { localizedCoreString(bridgePackagingKey(packaging: $0)) },
            edit: { .packaging(packaging: $0) },
            write: write
        )
    }
}
