import BaeKit
import SwiftUI

/// What Discogs says about the pressing that no other field holds: each
/// detail removable on its own, and a menu adding one more.
struct DiscogsDetailsEditor: View {
    let details: [BridgeDiscogsDetail]
    /// The pressing holds no detail, as core says, so the control that adds
    /// one stands alone.
    let isEmpty: Bool
    /// The details the pressing does not hold yet, as core offers them.
    let offered: [BridgeDiscogsDetail]
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        FlowLayout(
            spacing: ThemeSpace.related + ThemeSpace.line,
            rowSpacing: ThemeSpace.compact
        ) {
            ForEach(details, id: \.self) { detail in
                HStack(spacing: ThemeSpace.inline) {
                    let name = bridgeDiscogsDetailLabel(detail: detail).text
                    Text(name)
                        .themeText(.body)
                    MacIconButton.remove(name) {
                        commit(.removeDiscogsDetail(detail: detail))
                    }
                }
            }
            MacButton(
                kind: .plain,
                menu: {
                    for detail in offered {
                        MacMenuItem(
                            verbatim: bridgeDiscogsDetailLabel(detail: detail)
                                .text
                        ) { commit(.addDiscogsDetail(detail: detail)) }
                    }
                }
            ) {
                AddEntryLabel(
                    words: isEmpty
                        ? String(localized: "Add detail")
                        : String(localized: "Add")
                )
            }
            .fixedSize()
            .accessibilityLabel(String(localized: "Add detail"))
        }
    }

    /// Sends the one detail added or removed, for core to apply to the
    /// details it holds.
    private func commit(_ edit: BridgePressingFactEdit) {
        Task { await write(edit) }
    }
}
