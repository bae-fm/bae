import SwiftUI

struct AboutSettingsTab: View {
    let canCheckForUpdates: Bool
    let onCheckForUpdates: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Text(verbatim: "bae")
                .font(.largeTitle)
                .fontWeight(.bold)
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
                    .font(.caption.monospaced())
                    .foregroundStyle(.tertiary)
            }
            Button("Check for Updates...") {
                onCheckForUpdates()
            }
            .disabled(!canCheckForUpdates)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}

#Preview("About") {
    AboutSettingsTab(
        canCheckForUpdates: true,
        onCheckForUpdates: {},
    )
    .frame(width: 500, height: 300)
}
