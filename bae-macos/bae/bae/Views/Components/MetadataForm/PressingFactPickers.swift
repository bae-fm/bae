import BaeKit
import SwiftUI

// The controls that choose what a pressing is offer only the values bae's
// vocabulary has, so the form holds nothing a catalog does not state.

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
        MacMenuButton {
            MacMenuItem("Not stated", checked: area == nil) { choose(nil) }
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
        } label: {
            FactMenuLabel(text: area?.text)
        }
        .fixedSize()
    }

    private func choose(_ area: BridgeReleaseArea?) {
        Task { await write(.area(area: area)) }
    }
}

/// What the pressing is made of: each carrier by name with its count shown
/// and stepped, a control removing it, and a menu adding one more carrier.
struct MediaCountsEditor: View {
    let media: [BridgeMediaCount]
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ForEach(media, id: \.medium) { counted in
                MediumCountRow(
                    counted: counted,
                    setCount: { replace(counted.medium, count: $0) },
                    remove: { remove(counted.medium) }
                )
            }
            MacMenuButton {
                for medium in bridgeMedia()
                    .filter({ medium in
                        !media.contains { $0.medium == medium }
                    })
                {
                    MacMenuItem(
                        verbatim: bridgeMediumLabel(medium: medium).text
                    ) {
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
            .fixedSize()
        }
    }

    private func replace(_ medium: BridgeMedium, count: UInt32) {
        commit(
            media.map {
                $0.medium == medium
                    ? BridgeMediaCount(medium: medium, count: count) : $0
            }
        )
    }

    private func remove(_ medium: BridgeMedium) {
        commit(media.filter { $0.medium != medium })
    }

    private func commit(_ media: [BridgeMediaCount]) {
        Task { await write(.media(media: media)) }
    }
}

/// One carrier of the pressing: its name, its count with a stepper, and its
/// own remove button, since nobody looks for removal at a count of zero.
private struct MediumCountRow: View {
    let counted: BridgeMediaCount
    let setCount: (UInt32) -> Void
    let remove: () -> Void

    private var name: String {
        bridgeMediumLabel(medium: counted.medium).text
    }

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(name)
            Text(verbatim: "\u{00D7}\(counted.count.formatted())")
                .monospacedDigit()
                .foregroundStyle(.secondary)
            Stepper(
                value: Binding(
                    get: { Int(counted.count) },
                    set: { setCount(UInt32($0)) }
                ),
                in: 1...99
            ) {
                EmptyView()
            }
            .labelsHidden()
            .accessibilityLabel(String(localized: "Number of \(name)"))
            Button(action: remove) {
                Image(systemName: "xmark.circle.fill")
                    .foregroundStyle(.tertiary)
            }
            .buttonStyle(.accentPlain)
            .help(String(localized: "Remove \(name)"))
            .accessibilityLabel(String(localized: "Remove \(name)"))
        }
        .themeText(.body)
        .fixedSize()
    }
}

/// How official the release is.
struct ReleaseStatusPicker: View {
    let status: BridgeReleaseStatus?
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        MacMenuButton {
            MacMenuItem("Not stated", checked: status == nil) { choose(nil) }
            for option in bridgeReleaseStatuses() {
                MacMenuItem(
                    verbatim: Self.text(option),
                    checked: status == option
                ) { choose(option) }
            }
        } label: {
            FactMenuLabel(text: status.map(Self.text))
        }
        .fixedSize()
    }

    static func text(_ status: BridgeReleaseStatus) -> String {
        localizedCoreString(bridgeReleaseStatusKey(status: status))
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
        MacMenuButton {
            MacMenuItem("Not stated", checked: packaging == nil) {
                choose(nil)
            }
            for option in bridgePackagings() {
                MacMenuItem(
                    verbatim: Self.text(option),
                    checked: packaging == option
                ) { choose(option) }
            }
        } label: {
            FactMenuLabel(text: packaging.map(Self.text))
        }
        .fixedSize()
    }

    static func text(_ packaging: BridgePackaging) -> String {
        localizedCoreString(bridgePackagingKey(packaging: packaging))
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
        HStack(spacing: ThemeSpace.compact) {
            ForEach(details, id: \.self) { detail in
                HStack(spacing: ThemeSpace.inline) {
                    Text(bridgeDiscogsDetailLabel(detail: detail).text)
                        .themeText(.body)
                    Button {
                        commit(details.filter { $0 != detail })
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.tertiary)
                    }
                    .buttonStyle(.accentPlain)
                    .accessibilityLabel(String(localized: "Remove"))
                }
            }
            MacMenuButton {
                for detail in bridgeDiscogsDetails()
                    .filter({
                        !details.contains($0)
                    })
                {
                    MacMenuItem(
                        verbatim: bridgeDiscogsDetailLabel(detail: detail).text
                    ) { commit(details + [detail]) }
                }
            } label: {
                FactMenuLabel(
                    text: details.isEmpty ? nil : String(localized: "Add"),
                    systemImage: details.isEmpty ? nil : "plus"
                )
            }
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
        HStack(spacing: ThemeSpace.inline) {
            if let systemImage {
                Image(systemName: systemImage)
            }
            Text(text ?? "\u{2014}")
        }
        .themeText(.body)
        .foregroundStyle(text == nil ? .tertiary : .primary)
    }
}
