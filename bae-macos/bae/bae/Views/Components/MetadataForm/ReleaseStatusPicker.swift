import BaeKit
import SwiftUI

/// How official the release is.
struct ReleaseStatusPicker: View {
    let status: BridgeReleaseStatus?
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                MacMenuItem("Not stated", checked: status == nil) {
                    choose(nil)
                }
                for option in bridgeReleaseStatuses() {
                    MacMenuItem(
                        verbatim: Self.text(option),
                        checked: status == option
                    ) { choose(option) }
                }
            }
        ) {
            FactMenuLabel(text: status.map(Self.text))
        }
        .fixedSize()
        .accessibilityValue(
            Text(verbatim: FactMenuLabel.shown(status.map(Self.text)))
        )
    }

    static func text(_ status: BridgeReleaseStatus) -> String {
        localizedCoreString(bridgeReleaseStatusKey(status: status))
    }

    private func choose(_ status: BridgeReleaseStatus?) {
        Task { await write(.status(status: status)) }
    }
}
