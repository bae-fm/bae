import BaeKit
import SwiftUI

/// One CUE FILE reference's audio choices, already validated by core.
struct ImportSheetBindingMenu: View {
    let reference: BridgeSheetReferenceOptions
    let onBind: (String?) -> Void

    @State
    private var hovering = false

    var body: some View {
        Menu {
            ForEach(reference.options, id: \.fileId) { option in
                bindButton(option)
            }
            Divider()
            Button {
                onBind(nil)
            } label: {
                checkable(
                    coreString("ui.import.sheet.describes_nothing"),
                    selected: reference.fileId == nil
                )
            }
        } label: {
            Text(
                reference.fileId
                    ?? coreString("ui.import.sheet.choose_audio")
            )
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(
                reference.fileId == nil ? Color.orange : Theme.accent
            )
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
    private func bindButton(
        _ option: BridgeSheetBindingOption
    ) -> some View {
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
                    selected: reference.fileId == option.fileId
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
