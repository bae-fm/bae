import BaeKit
import SwiftUI

struct AboutSettingsTab: View {
    let canCheckForUpdates: Bool
    let onCheckForUpdates: () -> Void

    var body: some View {
        SettingsPane(.about) {
            content
        }
    }

    private var content: some View {
        VStack(spacing: ThemeSpace.related) {
            Logo(size: 96, bob: false, glow: false)
            if let version = Bundle.main.infoDictionary?[
                "CFBundleShortVersionString"
            ] as? String {
                Text("Version \(version)")
                    .foregroundStyle(.secondary)
            }
            if let commit = Bundle.main.infoDictionary?["BaeGitCommit"]
                as? String
            {
                Text(commit)
                    .themeText(.mono)
                    .foregroundStyle(.tertiary)
            }
            Button("Check for updates…") {
                onCheckForUpdates()
            }
            .disabled(!canCheckForUpdates)
        }
        .padding(.top, ThemeSpace.page + ThemeSpace.related)
        .frame(maxWidth: .infinity, alignment: .top)
    }
}
