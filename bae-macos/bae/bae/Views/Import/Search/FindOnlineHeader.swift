import BaeKit
import SwiftUI

/// The Find online title row: the way back, the page's name, and at its right
/// end one checkbox per metadata source. What identification and the search
/// have to say about themselves is on their own section headers below, each
/// beside the results it produced.
///
/// The checkboxes are the library's setting, not this candidate's, so the row
/// reads and writes them itself rather than taking them from whoever placed
/// it: the same switches sit in Settings › Import, and both go through core.
/// They are here because this is the page where a slow source is worth
/// dropping — uncheck it and every run and search from here on asks the rest,
/// while the run in front of you re-asks them straight away.
struct FindOnlineHeader: View {
    /// Leave the pane. `nil` for a surface that owns its own way out — the
    /// re-identify sheet closes rather than going back.
    let onBack: (() -> Void)?

    /// Which sources this library asks. Core's answer, read live off the
    /// config the app observes, so adding a Discogs token in Settings frees
    /// its checkbox while the page is open.
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Importer.self)
    private var importer
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        HStack(spacing: 12) {
            if let onBack {
                Button(action: onBack) {
                    Label("Back", systemImage: "chevron.left")
                }
                .buttonStyle(.link)
                .font(.system(size: 13))
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(width: 1, height: 14)
            }
            Text("Find online")
                .font(.system(size: 13, weight: .semibold))
            Spacer(minLength: 12)
            ForEach(configStore.config.metadataSources, id: \.source) {
                setting in
                sourceToggle(setting)
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 42)
    }

    /// One source's checkbox. Whether it can be moved is core's answer, not
    /// this view's: a source with no credential and the only source left being
    /// asked are both writes core would turn down.
    private func sourceToggle(
        _ setting: BridgeMetadataSourceSetting
    ) -> some View {
        Toggle(
            isOn: Binding(
                get: { setting.availability == .on },
                set: { setSourceEnabled(setting.source, $0) }
            )
        ) {
            Text(verbatim: bridgeMetadataSourceName(source: setting.source))
                .font(.system(size: 12))
        }
        .toggleStyle(.checkbox)
        .controlSize(.small)
        .disabled(!setting.canChange)
    }

    /// Write what the checkbox shows — an absolute value, not a flip — and
    /// report a refusal the way every other failed write does. Core refuses
    /// only when this would leave nothing to ask, and that checkbox is already
    /// disabled, so a refusal here is two windows racing.
    private func setSourceEnabled(
        _ source: BridgeMetadataSource,
        _ enabled: Bool
    ) {
        do {
            try importer.setMetadataSourceEnabled(source, enabled)
        }
        catch {
            uiStore.showError(error)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Header") {
        VStack(spacing: 0) {
            FindOnlineHeader(onBack: {})
            Divider()
            FindOnlineHeader(onBack: nil)
        }
        .frame(width: 660)
        .environment(PreviewData.configStore())
        .environment(Importer.stub())
        .environment(UiStore())
        .windowBackground()
    }

    #Preview("Header — one source switched off") {
        FindOnlineHeader(onBack: {})
            .frame(width: 660)
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    musicBrainz: .off
                )
            )
            .environment(Importer.stub())
            .environment(UiStore())
            .windowBackground()
    }

    #Preview("Header — Discogs has no token") {
        FindOnlineHeader(onBack: {})
            .frame(width: 660)
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    discogsUsable: false
                )
            )
            .environment(Importer.stub())
            .environment(UiStore())
            .windowBackground()
    }
#endif
