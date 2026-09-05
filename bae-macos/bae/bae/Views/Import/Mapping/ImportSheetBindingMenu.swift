import BaeKit
import SwiftUI

/// The sheet caption's binding control: what audio this track sheet describes,
/// or that it describes nothing.
///
/// The scan proposes a pairing from the sheet's `FILE` directive; when the
/// directive names a file that was later re-encoded under another name, the
/// user is the only one who knows the answer, and this is where they give it.
/// Core decides what may be offered — it probes each file — so this places the
/// answer rather than working one out.
///
/// The label is the audio's name and nothing else: no caret, so the line reads
/// "sheet → audio" and the menu is found by the hover. Why a sheet is on
/// nothing — the directive's own text, or the codec bae cannot carve — is the
/// label's tooltip.
struct ImportSheetBindingMenu: View {
    let sheet: BridgeSheetGroup
    /// The audio this sheet may be bound to, each already offered or refused by
    /// core.
    let options: [BridgeSheetBindingOption]
    /// Name the audio this sheet describes, or `nil` to leave it describing
    /// nothing.
    let onBind: (String?) -> Void

    @State
    private var hovering = false

    @ViewBuilder
    var body: some View {
        if let reason = sheet.bound.reasonLine {
            menu.help(reason)
        }
        else {
            menu
        }
    }

    private var menu: some View {
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
            Text(
                sheet.bound.containerName
                    ?? coreString("ui.import.sheet.choose_audio")
            )
            .font(.system(size: 11, design: .monospaced))
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(
                Color.primary.opacity(hovering ? 0.07 : 0),
                in: RoundedRectangle(cornerRadius: 4)
            )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .frame(minWidth: 24)
        .onHover { hovering = $0 }
    }

    /// One offered file, or a refused one shown disabled with core's reason —
    /// visible rather than hidden, so a folder whose only audio the sheet can't
    /// use reads as "here is why" instead of an empty menu.
    @ViewBuilder
    private func bindButton(_ option: BridgeSheetBindingOption) -> some View {
        if let refusal = option.refusalLine {
            Button {
            } label: {
                Text(verbatim: "\(option.fileId): \(refusal)")
                    .lineLimit(1)
                    .truncationMode(.middle)
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
                .lineLimit(1)
                .truncationMode(.middle)
        }
        else {
            Text(label)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }
}
