import BaeKit
import SwiftUI

/// What the pressing is made of: each carrier by name with its count shown
/// and stepped, a control removing it, and a menu adding one more carrier.
struct MediaCountsEditor: View {
    let media: [BridgeMediaCount]
    /// The media the pressing does not hold yet, as core offers them.
    let offered: [BridgeMedium]
    let write: @MainActor (BridgePressingFactEdit) async -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ForEach(media, id: \.medium) { counted in
                MediumCountRow(
                    counted: counted,
                    setCount: { commit(counted.medium, count: $0) },
                    remove: { commit(counted.medium, count: 0) }
                )
            }
            MacButton(
                kind: .plain,
                menu: {
                    for medium in offered {
                        MacMenuItem(
                            verbatim: bridgeMediumLabel(medium: medium).text
                        ) {
                            commit(medium, count: 1)
                        }
                    }
                }
            ) {
                AddEntryLabel(words: String(localized: "Add"))
            }
            .fixedSize()
            .accessibilityLabel(String(localized: "Add medium"))
        }
    }

    /// Sends the one medium's new count, with zero removing it, for core to
    /// apply to the media it holds.
    private func commit(_ medium: BridgeMedium, count: UInt32) {
        Task {
            await write(
                .mediumCount(
                    mediaCount: BridgeMediaCount(medium: medium, count: count)
                )
            )
        }
    }
}

/// One carrier of the pressing: its name, its count with a stepper, and its
/// own remove button, since nobody looks for removal at a count of zero.
private struct MediumCountRow: View {
    let counted: BridgeMediaCount
    let setCount: (UInt32) -> Void
    let remove: () -> Void

    private var name: String {
        bridgeMediumLabel(medium: counted.medium).text
    }

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(name)
            Text("\u{00D7}\(Int(counted.count))")
                .monospacedDigit()
                .foregroundStyle(Theme.macTextSecondary)
            MacStepper(
                label: String(localized: "Number of \(name)"),
                value: Binding(
                    get: { Int(counted.count) },
                    set: { setCount(UInt32($0)) }
                )
            )
            MacIconButton.remove(name, action: remove)
        }
        .themeText(.body)
        .fixedSize()
    }
}
