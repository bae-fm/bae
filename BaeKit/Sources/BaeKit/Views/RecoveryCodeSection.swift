import SwiftUI

/// The person's recovery code, for when they lose every device: hidden until
/// they ask, then shown inline as coven formats it, to copy or hide again.
/// Adding a device while one is at hand goes through Add Device in People
/// instead.
///
/// The screen holding the section owns `reveal` and hides it when the screen
/// goes away: a row of a lazy list leaves and returns as it scrolls, so the
/// section can't tell that apart from the screen closing.
public struct RecoveryCodeSection: View {
    let reveal: RecoveryCodeReveal

    @Environment(Sync.self)
    private var sync
    @Environment(ConfigStore.self)
    private var configStore

    public init(reveal: RecoveryCodeReveal) {
        self.reveal = reveal
    }

    /// An S3 library's bucket opens only with an access key, which the code
    /// doesn't carry.
    private var needsS3Key: Bool {
        if case .s3 = configStore.storage { return true }
        return false
    }

    public var body: some View {
        Section("Your recovery code") {
            Text(
                "If you lose all your devices, enter this code on a new one to get your library back. Anyone who has it and access to your storage can open the library, so keep it somewhere private."
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            if needsS3Key {
                Text("You'll also need your S3 access key and secret.")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            switch reveal.state {
            case .hidden:
                showButton
            case .loading:
                ProgressView()
                    .frame(maxWidth: .infinity)
            case .shown(let code):
                Text(code)
                    .themeText(.mono)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: ThemeSpace.group) {
                    Button("Copy") { Pasteboard.copy(code) }
                    Button("Hide") { reveal.hide() }
                }
                // Borderless, so a list row holding both buttons sends each
                // tap to the button under it.
                .buttonStyle(.borderless)
            case .failed(let error):
                ErrorDetailDisclosure(error: error)
                showButton
            }
        }
    }

    private var showButton: some View {
        Button("Show Code") { reveal.show(sync.generateRestoreCode) }
    }
}
