import BaeKit
import SwiftUI

/// Import metadata defaults and the sources Find online asks. Every control
/// writes through core; the config value stream redraws the stored value. The
/// Discogs key sits under the Discogs switch, because the switch cannot be
/// moved without one.
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
                Toggle("Pre-fill with tags", isOn: prefillWithTags)
                Toggle("Identify automatically", isOn: identifyAutomatically)
            } header: {
                Text("Metadata")
            } footer: {
                VStack(alignment: .leading, spacing: 6) {
                    Text(
                        "New candidates start from a draft read from their files' tags."
                    )
                    Text("New candidates are identified as they are added.")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section {
                ForEach(sourceSwitches.beforeKey, id: \.source) { setting in
                    sourceToggle(setting)
                }
                if let afterKey = sourceSwitches.afterKey {
                    DiscogsKeySection()
                    ForEach(afterKey, id: \.source) { setting in
                        sourceToggle(setting)
                    }
                }
            } header: {
                Text("Sources")
            } footer: {
                Text(
                    "Find online asks the sources that are checked here. The same checkboxes are on the Find online header."
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
            beforeKey: [BridgeMetadataSourceSetting],
            afterKey: [BridgeMetadataSourceSetting]?
        )
    {
        let settings = configStore.config.metadataSources
        guard
            let discogs = settings.firstIndex(where: { $0.source == .discogs })
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
        _ setting: BridgeMetadataSourceSetting
    ) -> some View {
        Toggle(
            String(
                localized:
                    "Search \(bridgeMetadataSourceName(source: setting.source))"
            ),
            isOn: Binding(
                get: { setting.availability == .on },
                set: { enabled in
                    do {
                        try importer.setMetadataSourceEnabled(
                            setting.source,
                            enabled
                        )
                    }
                    catch {
                        uiStore.showError(error)
                    }
                }
            )
        )
        .disabled(!setting.canChange)
    }

    private var identifyAutomatically: Binding<Bool> {
        Binding(
            get: { configStore.config.identifyAutomatically },
            set: { enabled in
                do {
                    try importer.setIdentifyAutomatically(enabled)
                }
                catch {
                    uiStore.showError(error)
                }
            }
        )
    }

    private var prefillWithTags: Binding<Bool> {
        Binding(
            get: { configStore.config.prefillWithTags },
            set: { enabled in
                do {
                    try importer.setPrefillWithTags(enabled)
                }
                catch {
                    uiStore.showError(error)
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
