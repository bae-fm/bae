import SwiftUI

/// The person's restore code, for when they lose every device: held masked
/// while Settings is open, with Show and Hide, Copy, and Print… where the app
/// can print. Adding a device while one is at hand goes through Add Device in
/// People instead.
///
/// The screen holding the section owns `reveal`, opens it when the screen
/// opens and closes it when the screen goes away: a row of a lazy list leaves
/// and returns as it scrolls, so the section can't tell that apart from the
/// screen closing.
public struct RecoveryCodeSection: View {
    let reveal: RecoveryCodeReveal
    /// Prints the code, given in groups of four; nil where the app can't.
    let onPrint: ((String) -> Void)?

    @Environment(ConfigStore.self)
    private var configStore

    @State
    private var copied = false

    public init(
        reveal: RecoveryCodeReveal,
        onPrint: ((String) -> Void)? = nil
    ) {
        self.reveal = reveal
        self.onPrint = onPrint
    }

    /// An S3 library's bucket opens only with an access key, which the code
    /// doesn't carry.
    private var needsS3Key: Bool {
        if case .s3 = configStore.storage { return true }
        return false
    }

    public var body: some View {
        Section("Restore code") {
            switch reveal.state {
            case .closed:
                EmptyView()
            case .loading:
                WaitingIndicator("Making your restore code…")
                    .frame(maxWidth: .infinity)
            case .masked(let code):
                codeRow(code, shown: false)
                caption
            case .shown(let code):
                codeRow(code, shown: true)
                caption
            case .failed(let error):
                ErrorDetailDisclosure(error: error)
            }
        }
    }

    private func codeRow(_ code: String, shown: Bool) -> some View {
        HStack(spacing: ThemeSpace.related) {
            if shown {
                Text(code)
                    .themeText(.mono)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            else {
                let parts = RestoreCodeMask.masked(code)
                HStack(spacing: ThemeSpace.inline) {
                    Text(parts.leading)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Text(parts.trailing)
                        .layoutPriority(1)
                }
                .themeText(.mono)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityLabel("Restore code, partly hidden")
            }
            // Borderless, so a list row holding the buttons sends each tap to
            // the button under it.
            Button(shown ? "Hide" : "Show") {
                if shown {
                    reveal.hide()
                }
                else {
                    reveal.show()
                }
            }
            if let onPrint {
                Button("Print…") { onPrint(code) }
            }
            Button {
                Pasteboard.copy(code)
                copied = true
            } label: {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
            }
            .accessibilityLabel(copied ? "Copied" : "Copy")
            .help(copied ? "Copied" : "Copy")
        }
        .buttonStyle(.borderless)
    }

    private var caption: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            (Text(
                "Print this code or save it in a password manager. It gets you back into your library if you lose all your devices."
            ) + Text(verbatim: " ")
                + Text("It’s also in your iCloud Keychain."))
                .themeText(.detail)
                .foregroundStyle(.secondary)
            if needsS3Key {
                Text("You'll also need your S3 access key and secret.")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
        }
    }
}
