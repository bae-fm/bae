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

    var body: some View {
        Form {
            Section {
                Toggle("Identify automatically", isOn: identifyAutomatically)
                // Only an automatic run imports on its own.
                Toggle(
                    "Import automatically when identified",
                    isOn: importWhenIdentified
                )
                .disabled(!configStore.config.identifyAutomatically)
            } header: {
                Text("Metadata")
            } footer: {
                VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                    Text("New candidates are identified as they are added.")
                    Text(
                        "A candidate identified from then on that needs nothing from you is imported right away, where your last import went."
                    )
                }
                .themeText(.detail)
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
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
    }

    /// The source switches split after the Discogs switch, where its key row
    /// goes; `afterKey` is nil when there is no Discogs source.
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

    /// One source's checkbox; core decides whether it can be moved.
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

#if DEBUG
    /// The settings window the previews draw in.
    private enum PreviewWindow {
        static let width: CGFloat = 500
        static let height: CGFloat = 500
    }

    #Preview("Import Settings") {
        ImportSettingsTab()
            .environment(PreviewData.configStore())
            .environment(Discogs.stub())
            .environment(PreviewData.importTabImporter())
            .environment(UiStore())
            .frame(width: PreviewWindow.width, height: PreviewWindow.height)
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
            .frame(width: PreviewWindow.width, height: PreviewWindow.height)
    }
#endif
