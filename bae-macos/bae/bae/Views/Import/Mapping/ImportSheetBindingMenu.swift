import BaeKit
import SwiftUI

/// The audio choices core offers for one CUE FILE reference with no audio
/// bound, as its own menu.
struct ImportSheetBindingMenu: View {
    let reference: BridgeSheetReferenceOptions
    let onBind: (String?) -> Void

    @State
    private var hovering = false

    var body: some View {
        Menu {
            ImportSheetBindingItems(reference: reference, onBind: onBind)
        } label: {
            Text(
                reference.fileId
                    ?? coreString("ui.import.sheet.choose_audio")
            )
            .themeText(.mono)
            .foregroundStyle(
                reference.fileId == nil ? Theme.warning : Theme.accent
            )
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background(
                hovering ? Theme.hover : Color.clear,
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .frame(minWidth: ThemeSize.hitTarget)
        .onHover { hovering = $0 }
    }
}

/// The items of one FILE reference's binding menu: each audio file core
/// offers or refuses for it, and clearing the binding.
struct ImportSheetBindingItems: View {
    let reference: BridgeSheetReferenceOptions
    let onBind: (String?) -> Void

    var body: some View {
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
    }

    /// An offered file, or a refused one shown disabled with core's reason so
    /// the menu explains itself instead of being empty.
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
