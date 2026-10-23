import BaeKit
import SwiftUI

/// The Import tab with no folder watched: Import Folder, which opens the
/// folder picker, and Add Discogs Key while Discogs can't be asked.
struct ImportEmptyState: View {
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            Text("Add a folder to import music from")
                .themeText(.body)
                .foregroundStyle(.secondary)
            HStack(spacing: ThemeSpace.related) {
                Button("Import Folder...") {
                    uiStore.setImportFolderPickerPresented(true)
                }
                AddDiscogsKeyButton()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

#if DEBUG
    #Preview("Import empty state \u{2014} no Discogs key") {
        ImportEmptyState()
            .environment(UiStore())
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    discogsUsable: false
                )
            )
            .environment(SettingsNavigation())
            .mainWindowPreviewEnvironment()
            .frame(width: 600, height: 400)
            .windowBackground()
    }

    #Preview("Import empty state \u{2014} Discogs key saved") {
        ImportEmptyState()
            .environment(UiStore())
            .environment(PreviewData.configStore())
            .environment(SettingsNavigation())
            .mainWindowPreviewEnvironment()
            .frame(width: 600, height: 400)
            .windowBackground()
    }
#endif
