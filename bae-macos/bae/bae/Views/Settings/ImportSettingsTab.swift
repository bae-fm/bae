import BaeKit
import SwiftUI

/// Import metadata defaults, what identification does, and the sources Find
/// online asks. Every control writes through core; the config value stream
/// redraws the stored value. The Discogs key sits under the Discogs switch,
/// because the switch cannot be moved without one.
struct ImportSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Importer.self)
    private var importer
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Form {
            Section {
                Toggle(
                    "Pre-fill from file metadata",
                    isOn: prefillWithFileMetadata
                )
                Toggle("Identify automatically", isOn: identifyAutomatically)
            } header: {
                Text("Metadata")
            } footer: {
                VStack(alignment: .leading, spacing: 6) {
                    Text(
                        "New candidates start from a draft read from their files, sheets and folder name."
                    )
                    Text("New candidates are identified as they are added.")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section {
                ForEach(
                    configStore.config.identificationSteps,
                    id: \.step
                ) { setting in
                    stepToggle(setting)
                }
            } header: {
                Text("Identification")
            } footer: {
                Text(
                    "A step that is off is skipped by every identification from then on, and says so where the run is shown."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section {
                ForEach(sourceSwitches.beforeKey, id: \.catalog) { setting in
                    sourceToggle(setting)
                }
                if let afterKey = sourceSwitches.afterKey {
                    DiscogsKeySection()
                    ForEach(afterKey, id: \.catalog) { setting in
                        sourceToggle(setting)
                    }
                }
            } header: {
                Text("Sources")
            } footer: {
                Text(
                    "Identification asks the sources that are checked here."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
    }

    /// The source switches split where the Discogs key belongs: the switches
    /// the key follows, then the switches after it. Core owns which sources
    /// exist and in what order, so the key follows the Discogs switch wherever
    /// that lands; `afterKey` is nil when the library has no Discogs source,
    /// which is when there is no key to draw at all.
    private var sourceSwitches:
        (
            beforeKey: [BridgeLookupCatalogSetting],
            afterKey: [BridgeLookupCatalogSetting]?
        )
    {
        let settings = configStore.config.lookupCatalogs
        guard
            let discogs = settings.firstIndex(where: { $0.catalog == .discogs })
        else {
            return (settings, nil)
        }
        return (
            Array(settings[...discogs]),
            Array(settings[settings.index(after: discogs)...])
        )
    }

    /// One source's checkbox. Whether it can be moved is core's answer, not
    /// this view's: a source with no credential and the only source left being
    /// asked are both writes core would turn down.
    private func sourceToggle(
        _ setting: BridgeLookupCatalogSetting
    ) -> some View {
        Toggle(
            String(
                localized:
                    "Search \(bridgeCatalogName(catalog: setting.catalog))"
            ),
            isOn: Binding(
                get: { setting.availability == .on },
                set: { enabled in
                    Task {
                        do {
                            try await importer.setMetadataSourceEnabled(
                                setting.catalog,
                                enabled
                            )
                        }
                        catch {
                            uiStore.showError(error)
                        }
                    }
                }
            )
        )
        .disabled(!setting.canChange)
    }

    /// One identification step's switch.
    private func stepToggle(
        _ setting: BridgeIdentificationStepSetting
    ) -> some View {
        Toggle(
            setting.step.settingLabel,
            isOn: Binding(
                get: { setting.enabled },
                set: { enabled in
                    write {
                        try await importer.setIdentificationStep(
                            setting.step,
                            enabled
                        )
                    }
                }
            )
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

    private var prefillWithFileMetadata: Binding<Bool> {
        Binding(
            get: { configStore.config.prefillWithFileMetadata },
            set: { enabled in
                Task {
                    do {
                        try await importer.setPrefillWithFileMetadata(enabled)
                    }
                    catch {
                        uiStore.showError(error)
                    }
                }
            }
        )
    }
}

#if DEBUG
    #Preview("Import Settings") {
        ImportSettingsTab()
            .environment(PreviewData.configStore())
            .environment(Discogs.stub())
            .environment(PreviewData.importTabImporter())
            .environment(UiStore())
            .frame(width: 500, height: 500)
    }

    #Preview("Import Settings, no Discogs key") {
        ImportSettingsTab()
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    discogsUsable: false
                )
            )
            .environment(Discogs.stub())
            .environment(PreviewData.importTabImporter())
            .environment(UiStore())
            .frame(width: 500, height: 500)
    }
#endif

extension BridgeIdentificationStep {
    /// The step as its switch in Settings names it.
    var settingLabel: String {
        switch self {
        case .readCoverArt: String(localized: "Read cover art")
        case .lookUpDiscIds: String(localized: "Look up disc IDs")
        case .lookUpBarcodes: String(localized: "Look up barcodes")
        case .searchByTitle: String(localized: "Search by title")
        case .followCatalogLinks:
            String(localized: "Join records across catalogs")
        }
    }
}
