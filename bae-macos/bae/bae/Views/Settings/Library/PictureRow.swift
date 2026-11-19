import AppKit
import BaeKit
import SwiftUI
import UniformTypeIdentifiers

/// The Picture row of You: this person's picture, or their initials while
/// they have none, with Choose…, which opens the system's panel for picking an
/// image, and Remove, shown while they have a picture. The picked file's bytes
/// go to bae, which makes the avatar from them; the row keeps drawing the
/// current picture until the write returns.
struct PictureRow: View {
    let profile: BridgePersonProfile
    /// Whether another change of the page is still running.
    let busy: Bool
    /// Runs a write of the page, which shows its failure.
    let run: (@escaping () async throws -> Void) -> Void

    @Environment(Sync.self)
    private var sync

    var body: some View {
        MacRow {
            HStack(spacing: ThemeSpace.related) {
                if profile.avatarVersion != nil {
                    Button("Remove") {
                        run { try await sync.setAvatar(nil) }
                    }
                    .disabled(busy)
                }
                Button("Choose…") { choose() }
                    .disabled(busy)
            }
        } label: {
            HStack(spacing: ThemeSpace.related) {
                Avatar(profile: profile, size: 28)
                Text("Picture")
            }
        }
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        run {
            let bytes = try await Task.detached { try Data(contentsOf: url) }
                .value
            try await sync.setAvatar(bytes)
        }
    }
}
