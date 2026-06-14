import SwiftUI

struct PlaybackSettingsTab: View {
    @AppStorage("persistPlayback")
    private var persistPlayback = false

    var body: some View {
        Form {
            Section {
                Toggle("Restore on launch", isOn: $persistPlayback)
            } footer: {
                Text(
                    "Saves the current track, position, queue, and volume on quit and restores them on next launch."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
    }
}

#Preview("Playback Settings") {
    PlaybackSettingsTab()
        .frame(width: 500, height: 300)
}
