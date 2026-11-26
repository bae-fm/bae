import SwiftUI

/// The person's restore code, for when they lose every device: held masked
/// while Settings is open, with Show and Hide, Copy, and Print…. Adding a device while one is at hand goes through Add Device in
/// People instead.
///
/// The screen holding the section owns `reveal`, opens it when the screen
/// opens and closes it when the screen goes away: the section itself can't
/// tell the screen closing from its own view leaving and returning.
public struct MacRestoreCode<Header: View>: View {
    let reveal: RestoreCodeReveal
    /// Prints the code, given in groups of four.
    let onPrint: (String) -> Void
    /// The screen's heading over the section, which the box and notes under
    /// it don't draw themselves.
    let header: Header

    @Environment(ConfigStore.self)
    private var configStore

    @State
    private var copied = false

    public init(
        reveal: RestoreCodeReveal,
        onPrint: @escaping (String) -> Void,
        @ViewBuilder header: () -> Header
    ) {
        self.reveal = reveal
        self.onPrint = onPrint
        self.header = header()
    }

    /// An S3 library's bucket opens only with an access key, which the code
    /// doesn't carry.
    private var needsS3Key: Bool {
        if case .s3 = configStore.storage { return true }
        return false
    }

    public var body: some View {
        MacSection {
            switch reveal.state {
            case .closed:
                EmptyView()
            case .loading:
                MacSpinner("Making your restore code…")
                    .frame(maxWidth: .infinity)
            case .masked(let share):
                codeRow(share, shown: false)
            case .shown(let share):
                codeRow(share, shown: true)
            case .failed(let error):
                MacNotice(error: error)
            }
        } header: {
            header
        } footer: {
            switch reveal.state {
            case .masked, .shown:
                // The boards put 4 points over the first note and between
                // the notes, as they do under the code's box.
                caption
                    .footerNoteSpace(ThemeSpace.line * 2)
            case .closed, .loading, .failed:
                EmptyView()
            }
        }
    }

    private func codeRow(_ share: BridgeRestoreCodeShare, shown: Bool)
        -> some View
    {
        HStack(spacing: ThemeSpace.related) {
            if shown {
                Text(Self.spacedGroups(share.code))
                    .selectableCode(share.code)
                    // After `selectableCode`, so the text VoiceOver reads and
                    // the shots measure takes the mono font too.
                    .themeText(.mono)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            else {
                // The last group stays whole when the row is too narrow for
                // every group.
                HStack(spacing: ThemeSpace.inline) {
                    Text(
                        share.maskedGroups.dropLast().joined(separator: " ")
                    )
                    .lineLimit(1)
                    .truncationMode(.tail)
                    // The groups take the room the last group leaves, so the
                    // last group ends the row, as the board's does.
                    .frame(maxWidth: .infinity, alignment: .leading)
                    if let last = share.maskedGroups.last {
                        Text(last)
                            .layoutPriority(1)
                    }
                }
                .themeText(.mono)
                .foregroundStyle(Theme.textSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                // A picture of the code, not words to read out: the dots
                // say nothing, and the code stays hidden until Show.
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Restore code, partly hidden")
                .accessibilityAddTraits(.isImage)
            }
            MacButton(shown ? "Hide" : "Show") {
                if shown {
                    reveal.hide()
                }
                else {
                    reveal.show()
                }
            }
            MacButton("Print…") { onPrint(share.code) }
            MacButton(kind: .icon) {
                Pasteboard.copy(share.code)
                copied = true
            } label: {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
            }
            .accessibilityLabel(copied ? "Copied" : "Copy")
            .help(copied ? "Copied" : "Copy")
        }
    }

    /// The code with 4 points more after each space, as the boards' word
    /// spacing sets its groups apart.
    private static func spacedGroups(_ code: String) -> AttributedString {
        var spaced = AttributedString(code)
        var index = spaced.startIndex
        while index < spaced.endIndex {
            let next = spaced.index(afterCharacter: index)
            if spaced[index..<next].characters.first == " " {
                spaced[index..<next].kern = 4
            }
            index = next
        }
        return spaced
    }

    /// The notes under the code. The form that draws them as its footer sets
    /// their text style once, over both.
    private var caption: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line * 2) {
            Text(
                "Print this code or save it in a password manager. It gets you back into your library if you lose all your devices."
            )
            if needsS3Key {
                Text("You will also need your S3 access key and secret.")
            }
        }
    }
}
