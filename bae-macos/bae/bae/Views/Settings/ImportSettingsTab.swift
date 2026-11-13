import BaeKit
import SwiftUI

/// Import metadata defaults and the sources Find online asks; every control
/// writes through core and redraws from config.
struct ImportSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Importer.self)
    private var importer
    @Environment(UiStore.self)
    private var uiStore

    /// Set while the person is asked to confirm turning Discogs off, which
    /// clears its saved key.
    @State
    private var confirmsTurningDiscogsOff = false

    var body: some View {
        SettingsPane(.importing) {
            Form {
                Section {
                    SettingToggleRow(
                        title: "Identify automatically",
                        note: "Looks up each new album as soon as it is added.",
                        isOn: identifyAutomatically
                    )
                    ForEach(sourceSwitches.beforeKey, id: \.catalog) {
                        setting in
                        sourceToggle(setting)
                    }
                    if let discogs = sourceSwitches.discogs {
                        // The key belongs to the Discogs switch above it.
                        DiscogsKeySection(isEnabled: isOn(discogs))
                            .joiningRowAbove()
                    }
                    ForEach(sourceSwitches.afterKey, id: \.catalog) { setting in
                        sourceToggle(setting)
                    }
                } footer: {
                    Text(
                        "Searching both MusicBrainz and Discogs helps bae recognise more of your albums automatically."
                    )
                }

                Section {
                    SettingToggleRow(
                        title: "Import automatically when identified",
                        note:
                            "Adds the album to your library once bae is sure of the match.",
                        isOn: importWhenIdentified
                    )
                    if configStore.syncs {
                        // Where an import goes belongs to the switch above it.
                        LabeledContent("Import to") {
                            MacImportDestination(destination: importDestination)
                        }
                        .joiningRowAbove()
                        .disabled(!configStore.config.importWhenIdentified)
                    }
                }
            }
            .formStyle(.settings)
        }
        .macAlert(isPresented: $confirmsTurningDiscogsOff) {
            TurnOffDiscogsConfirmation.alert {
                setSource(.discogs, enabled: false)
            }
        }
    }

    /// The source switches split after the Discogs switch, where its key rows
    /// go; with no Discogs source every switch is `beforeKey` and `discogs` is
    /// nil.
    private struct SourceSwitches {
        let beforeKey: [BridgeLookupCatalogSetting]
        let discogs: BridgeLookupCatalogSetting?
        let afterKey: [BridgeLookupCatalogSetting]
    }

    private var sourceSwitches: SourceSwitches {
        let settings = configStore.config.lookupCatalogs
        guard
            let discogs = settings.firstIndex(where: { $0.catalog == .discogs })
        else {
            return SourceSwitches(
                beforeKey: settings,
                discogs: nil,
                afterKey: []
            )
        }
        return SourceSwitches(
            beforeKey: Array(settings[...discogs]),
            discogs: settings[discogs],
            afterKey: Array(settings[settings.index(after: discogs)...])
        )
    }

    /// Whether the person has the source switched on, key or no key.
    private func isOn(_ setting: BridgeLookupCatalogSetting) -> Bool {
        setting.availability != .off
    }

    private func setSource(_ catalog: BridgeLookupCatalog, enabled: Bool) {
        Task {
            do {
                try await importer.setMetadataSourceEnabled(catalog, enabled)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    /// One source's switch; core decides whether it can be moved.
    @ViewBuilder
    private func sourceToggle(
        _ setting: BridgeLookupCatalogSetting
    ) -> some View {
        if setting.catalog == .discogs {
            discogsToggle(setting)
        }
        else {
            plainToggle(setting)
        }
    }

    private func plainToggle(
        _ setting: BridgeLookupCatalogSetting
    ) -> some View {
        SettingToggleRow(
            title: sourceTitle(setting.catalog),
            isOn: Binding(
                get: { isOn(setting) },
                set: { setSource(setting.catalog, enabled: $0) }
            ),
            canChange: setting.canChange
        ) {
            if setting.catalog == .musicBrainz {
                MusicBrainzInfoTip()
            }
        }
    }

    /// The Discogs switch: always usable without a key. Turning it off with
    /// a key saved asks first, since that clears the key.
    private func discogsToggle(
        _ setting: BridgeLookupCatalogSetting
    ) -> some View {
        let status = configStore.config.discogsTokenStatus
        return SettingToggleRow(
            title: sourceTitle(setting.catalog),
            note: isOn(setting) && status == .valid ? "Key saved" : nil,
            isOn: Binding(
                get: { isOn(setting) },
                set: { enabled in
                    if enabled || status == .notConfigured {
                        setSource(setting.catalog, enabled: enabled)
                    }
                    else {
                        confirmsTurningDiscogsOff = true
                    }
                }
            ),
            canChange: setting.canChange
        ) {
            DiscogsInfoTip()
        }
    }

    private func sourceTitle(_ catalog: BridgeLookupCatalog)
        -> LocalizedStringKey
    {
        "Search \(bridgeLookupCatalogName(catalog: catalog))"
    }

    private var importDestination: Binding<BridgeImportDestination> {
        Binding(
            get: { configStore.config.importDestination },
            set: { destination in
                write { try await importer.setImportDestination(destination) }
            }
        )
    }

    private var importWhenIdentified: Binding<Bool> {
        Binding(
            get: { configStore.config.importWhenIdentified },
            set: { enabled in
                write { try await importer.setImportWhenIdentified(enabled) }
            }
        )
    }

    private func write(_ write: @escaping @MainActor () async throws -> Void) {
        Task { @MainActor in
            do { try await write() }
            catch { uiStore.showError(error) }
        }
    }

    private var identifyAutomatically: Binding<Bool> {
        Binding(
            get: { configStore.config.identifyAutomatically },
            set: { enabled in
                Task {
                    do {
                        try await importer.setIdentifyAutomatically(enabled)
                    }
                    catch {
                        uiStore.showError(error)
                    }
                }
            }
        )
    }
}

/// The alert that asks before the Discogs source is turned off and its saved
/// key cleared.
enum TurnOffDiscogsConfirmation {
    /// The alert; Turn off runs `onTurnOff`.
    static func alert(
        onTurnOff: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text("Turn off Discogs?"),
                message: Text("Your saved key will be cleared.")
            ),
            [
                MacAlertButton(Text("Turn off"), action: onTurnOff),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }
}
