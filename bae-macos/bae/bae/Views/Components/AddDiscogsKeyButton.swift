import BaeKit
import SwiftUI

/// Add Discogs Key..., beside the first-import actions of an empty library
/// and an empty Import tab. It opens Settings on the Import pane, as the Find
/// online Discogs bar's Open Settings does: the key is entered under the
/// Discogs source switch there, and its field takes focus while no key is
/// stored.
///
/// Present while core says Discogs can't be used — no key, or one Discogs
/// rejected — the same condition the Find online bar shows on. Saving a key
/// changes the config core sends, so the button goes as soon as it is saved.
struct AddDiscogsKeyButton: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(SettingsNavigation.self)
    private var settingsNavigation
    @Environment(\.openSettingsWindow)
    private var openSettings

    var body: some View {
        if !configStore.config.discogsUsable {
            Button(
                "Add \(bridgeCatalogName(catalog: .discogs)) Key..."
            ) {
                settingsNavigation.open(
                    .importing,
                    present: { openSettings() }
                )
            }
        }
    }
}
