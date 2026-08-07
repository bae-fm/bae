import BaeKit
import SwiftUI

/// The sheet header's binding control: what audio this track sheet describes,
/// or that it describes nothing.
///
/// The scan proposes a pairing from the sheet's `FILE` directive; when the
/// directive names a file that was later re-encoded under another name, the
/// user is the only one who knows the answer, and this is where they give it.
/// Core decides what may be offered — it probes each file — so this places the
/// answer rather than working one out.
struct ImportSheetBindingMenu: View {
    let sheet: BridgeSheetGroup
    /// The audio this sheet may be bound to, each already offered or refused by
    /// core. `nil` until it has been asked for; empty means there is nothing to
    /// offer, so no menu appears.
    let options: [BridgeSheetBindingOption]?
    /// Name the audio this sheet describes, or `nil` to leave it describing
    /// nothing.
    let onBind: (String?) -> Void

    var body: some View {
        if let options, !options.isEmpty {
            Menu {
                ForEach(options, id: \.fileId) { option in
                    bindButton(option)
                }
                Divider()
                Button {
                    onBind(nil)
                } label: {
                    checkable(
                        coreString("ui.import.sheet.describes_nothing"),
                        selected: sheet.bound.containerId == nil
                    )
                }
            } label: {
                Label(
                    sheet.bound.containerName
                        ?? coreString("ui.import.sheet.choose_audio"),
                    systemImage: "link"
                )
                .font(.caption2)
                .lineLimit(1)
                .truncationMode(.middle)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .foregroundStyle(.secondary)
        }
    }

    /// One offered file, or a refused one shown disabled with core's reason —
    /// visible rather than hidden, so a folder whose only audio the sheet can't
    /// use reads as "here is why" instead of an empty menu.
    @ViewBuilder
    private func bindButton(_ option: BridgeSheetBindingOption) -> some View {
        if let refusal = option.refusalLine {
            Button {
            } label: {
                Text(verbatim: "\(option.fileId) — \(refusal)")
            }
            .disabled(true)
        }
        else {
            Button {
                onBind(option.fileId)
            } label: {
                checkable(
                    option.fileId,
                    selected: sheet.bound.containerId == option.fileId
                )
            }
        }
    }

    @ViewBuilder
    private func checkable(_ label: String, selected: Bool) -> some View {
        if selected {
            Label(label, systemImage: "checkmark")
        }
        else {
            Text(label)
        }
    }
}
