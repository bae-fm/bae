import BaeKit
import SwiftUI

/// Where the pressing was released: no answer, a country, or a region.
struct ReleaseAreaPicker: View {
    let area: BridgeReleaseArea?
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    /// Countries and regions, each sorted by its name in the reader's
    /// language — the order a person looks one up in.
    private static let countries: [BridgeReleaseArea] =
        bridgeCountryCodes()
        .map { BridgeReleaseArea.country(code: $0) }
        .sorted {
            $0.text.localizedStandardCompare($1.text) == .orderedAscending
        }
    private static let regions: [BridgeReleaseArea] =
        bridgeRegions()
        .map { BridgeReleaseArea.region(region: $0) }
        .sorted {
            $0.text.localizedStandardCompare($1.text) == .orderedAscending
        }

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                MacMenuItem(
                    verbatim: FactMenuLabel.shown(nil),
                    checked: area == nil
                ) { choose(nil) }
                MacMenuSection("Countries") {
                    for country in Self.countries {
                        MacMenuItem(
                            verbatim: country.text,
                            checked: area == country
                        ) { choose(country) }
                    }
                }
                MacMenuSection("Regions") {
                    for region in Self.regions {
                        MacMenuItem(
                            verbatim: region.text,
                            checked: area == region
                        ) { choose(region) }
                    }
                }
            }
        ) {
            FactMenuLabel(text: area?.text)
        }
        .fixedSize()
        .accessibilityValue(Text(verbatim: FactMenuLabel.shown(area?.text)))
    }

    private func choose(_ area: BridgeReleaseArea?) {
        Task { await write(.area(area: area)) }
    }
}
