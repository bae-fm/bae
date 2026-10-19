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

/// One entry of a FILE reference's binding menu. A reference's entries are
/// each audio file core offers or refuses for it, in core's order, then
/// binding none.
enum ImportSheetBindingChoice: Hashable {
    /// A file core offers; choosing it binds it to the reference.
    case offered(fileId: String, isBound: Bool)
    /// A file core refuses, with core's reason. It is listed, dimmed, so the
    /// menu explains itself instead of being empty, and is never chosen.
    case refused(fileId: String, reason: String)
    /// Binding no audio to the reference.
    case describesNothing(isBound: Bool)

    static func choices(
        for reference: BridgeSheetReferenceOptions
    ) -> [Self] {
        reference.options.map { option in
            if let reason = option.refusalLine {
                return .refused(fileId: option.fileId, reason: reason)
            }
            return .offered(
                fileId: option.fileId,
                isBound: reference.fileId == option.fileId
            )
        } + [.describesNothing(isBound: reference.fileId == nil)]
    }
}

/// The items of one FILE reference's binding menu, one per
/// `ImportSheetBindingChoice`.
struct ImportSheetBindingItems: View {
    let reference: BridgeSheetReferenceOptions
    let onBind: (String?) -> Void

    var body: some View {
        ForEach(
            ImportSheetBindingChoice.choices(for: reference),
            id: \.self
        ) { choice in
            switch choice {
            case .offered(let fileId, let isBound):
                Button {
                    onBind(fileId)
                } label: {
                    checkable(fileId, selected: isBound)
                }
            case .refused(let fileId, let reason):
                Button {
                } label: {
                    Text(verbatim: "\(fileId): \(reason)")
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .disabled(true)
            case .describesNothing(let isBound):
                Divider()
                Button {
                    onBind(nil)
                } label: {
                    checkable(
                        coreString("ui.import.sheet.describes_nothing"),
                        selected: isBound
                    )
                }
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
