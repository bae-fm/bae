import BaeKit
import SwiftUI

/// The controls that choose what a pressing is. Each offers only the values
/// bae's vocabulary has — countries by their ISO code, named in the reader's
/// language; regions, media, statuses, packagings and Discogs details by
/// their catalog words — so the form can hold nothing a catalog does not
/// state. A choice is handed to the writer whole; core keeps a medium only
/// while its count is above zero and a detail once.
enum PressingFactPickers {
    /// The face of every picker, matching the form's text fields.
    static let controlFont: Font = .system(size: 12.5)
}

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
        Menu {
            Button(String(localized: "Not stated")) {
                choose(nil)
            }
            Section(String(localized: "Countries")) {
                ForEach(Self.countries, id: \.self) { country in
                    Button(country.text) { choose(country) }
                }
            }
            Section(String(localized: "Regions")) {
                ForEach(Self.regions, id: \.self) { region in
                    Button(region.text) { choose(region) }
                }
            }
        } label: {
            FactMenuLabel(text: area?.text)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private func choose(_ area: BridgeReleaseArea?) {
        Task { await write(.area(area: area)) }
    }
}

/// What the pressing is made of: each carrier with a stepper for its count,
/// and a menu adding one more carrier.
struct MediaCountsEditor: View {
    let media: [BridgeMediaCount]
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        HStack(spacing: 10) {
            ForEach(media, id: \.medium) { counted in
                Stepper(
                    value: Binding(
                        get: { Int(counted.count) },
                        set: { count in
                            replace(
                                counted.medium,
                                count: UInt32(max(count, 0))
                            )
                        }
                    ),
                    in: 0...99
                ) {
                    Text(PressingText.media([counted]))
                        .font(PressingFactPickers.controlFont)
                }
                .fixedSize()
            }
            Menu {
                ForEach(
                    bridgeMedia()
                        .filter { medium in
                            !media.contains { $0.medium == medium }
                        },
                    id: \.self
                ) { medium in
                    Button(bridgeMediumLabel(medium: medium).text) {
                        commit(
                            media + [BridgeMediaCount(medium: medium, count: 1)]
                        )
                    }
                }
            } label: {
                FactMenuLabel(
                    text: media.isEmpty ? nil : String(localized: "Add"),
                    systemImage: media.isEmpty ? nil : "plus"
                )
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
        }
    }

    /// A count of zero drops the carrier; core leaves it out.
    private func replace(_ medium: BridgeMedium, count: UInt32) {
        commit(
            media.map {
                $0.medium == medium
                    ? BridgeMediaCount(medium: medium, count: count) : $0
            }
        )
    }

    private func commit(_ media: [BridgeMediaCount]) {
        Task { await write(.media(media: media)) }
    }
}

/// How official the release is.
struct ReleaseStatusPicker: View {
    let status: BridgeReleaseStatus?
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        Menu {
            Button(String(localized: "Not stated")) {
                choose(nil)
            }
            ForEach(bridgeReleaseStatuses(), id: \.self) { status in
                Button(Self.text(status)) { choose(status) }
            }
        } label: {
            FactMenuLabel(text: status.map(Self.text))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    static func text(_ status: BridgeReleaseStatus) -> String {
        QueueSummary.message(bridgeReleaseStatusKey(status: status))
    }

    private func choose(_ status: BridgeReleaseStatus?) {
        Task { await write(.status(status: status)) }
    }
}

/// What the release is sold in.
struct PackagingPicker: View {
    let packaging: BridgePackaging?
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        Menu {
            Button(String(localized: "Not stated")) {
                choose(nil)
            }
            ForEach(bridgePackagings(), id: \.self) { packaging in
                Button(Self.text(packaging)) { choose(packaging) }
            }
        } label: {
            FactMenuLabel(text: packaging.map(Self.text))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    static func text(_ packaging: BridgePackaging) -> String {
        QueueSummary.message(bridgePackagingKey(packaging: packaging))
    }

    private func choose(_ packaging: BridgePackaging?) {
        Task { await write(.packaging(packaging: packaging)) }
    }
}

/// What Discogs says about the pressing that no other field holds: each
/// detail removable on its own, and a menu adding one more.
struct DiscogsDetailsEditor: View {
    let details: [BridgeDiscogsDetail]
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        HStack(spacing: 6) {
            ForEach(details, id: \.self) { detail in
                HStack(spacing: 3) {
                    Text(bridgeDiscogsDetailLabel(detail: detail).text)
                        .font(PressingFactPickers.controlFont)
                    Button {
                        commit(details.filter { $0 != detail })
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.tertiary)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(String(localized: "Remove"))
                }
            }
            Menu {
                ForEach(
                    bridgeDiscogsDetails().filter { !details.contains($0) },
                    id: \.self
                ) { detail in
                    Button(bridgeDiscogsDetailLabel(detail: detail).text) {
                        commit(details + [detail])
                    }
                }
            } label: {
                FactMenuLabel(
                    text: details.isEmpty ? nil : String(localized: "Add"),
                    systemImage: details.isEmpty ? nil : "plus"
                )
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
        }
    }

    private func commit(_ details: [BridgeDiscogsDetail]) {
        Task { await write(.discogsDetails(discogsDetails: details)) }
    }
}

/// A picker's face: the chosen value, or the form's empty mark.
private struct FactMenuLabel: View {
    let text: String?
    var systemImage: String?

    var body: some View {
        HStack(spacing: 3) {
            if let systemImage {
                Image(systemName: systemImage)
            }
            Text(text ?? "\u{2014}")
        }
        .font(PressingFactPickers.controlFont)
        .foregroundStyle(text == nil ? .tertiary : .primary)
    }
}
