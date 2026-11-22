import BaeKit
import SwiftUI

/// What Discogs says about the pressing that no other field holds: each
/// detail removable on its own, and a menu adding one more.
struct DiscogsDetailsEditor: View {
    let details: [BridgeDiscogsDetail]
    /// The details the pressing does not hold yet, as core offers them.
    let offered: [BridgeDiscogsDetail]
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            ForEach(details, id: \.self) { detail in
                HStack(spacing: ThemeSpace.inline) {
                    let name = bridgeDiscogsDetailLabel(detail: detail).text
                    Text(name)
                        .themeText(.body)
                    MacIconButton.remove(name) {
                        commit(details.filter { $0 != detail })
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
                        ) { commit(details + [detail]) }
                    }
                }
            ) {
                if details.isEmpty {
                    FactMenuLabel(text: nil)
                }
                else {
                    AddEntryLabel(words: String(localized: "Add"))
                }
            }
            .fixedSize()
            .accessibilityLabel(
                details.isEmpty
                    ? String(localized: "Details")
                    : String(localized: "Add detail")
            )
        }
    }

    private func commit(_ details: [BridgeDiscogsDetail]) {
        Task { await write(.discogsDetails(discogsDetails: details)) }
    }
}
