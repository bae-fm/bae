import BaeKit
import SwiftUI

/// The audio choices core offers for the CUE FILE references with no audio
/// bound, as one menu on the sheet's caption: the choices directly for one
/// reference, a submenu per reference for several.
struct ImportSheetUnboundMenu: View {
    let references: [BridgeSheetReferenceOptions]
    let onBind: (BridgeSheetReferenceOptions, String?) -> Void

    @State
    private var hovering = false

    var body: some View {
        MacMenuButton {
            if let only = references.first, references.count == 1 {
                ImportSheetBindingItems(
                    reference: only,
                    onBind: { onBind(only, $0) }
                )
            }
            else {
                for reference in references {
                    MacMenuSubmenu(verbatim: reference.fileReference) {
                        ImportSheetBindingItems(
                            reference: reference,
                            onBind: { onBind(reference, $0) }
                        )
                    }
                }
            }
        } label: {
            Text(localizedCoreString("ui.import.sheet.choose_audio"))
                .themeText(.mono)
                .foregroundStyle(Theme.warning)
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .background(
                    hovering ? Theme.hover : Color.clear,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
                .frame(minWidth: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
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
struct ImportSheetBindingItems: MacMenuPart {
    let reference: BridgeSheetReferenceOptions
    let onBind: (String?) -> Void

    var entries: [MacMenuEntry] {
        var entries: [MacMenuEntry] = []
        for choice in ImportSheetBindingChoice.choices(for: reference) {
            switch choice {
            case .offered(let fileId, let isBound):
                entries.append(
                    .item(
                        MacMenuItem(verbatim: fileId, checked: isBound) {
                            onBind(fileId)
                        }
                    )
                )
            case .refused(let fileId, let reason):
                entries.append(
                    .item(
                        MacMenuItem(
                            verbatim: "\(fileId): \(reason)",
                            disabled: true
                        ) {}
                    )
                )
            case .describesNothing(let isBound):
                entries.append(.divider)
                entries.append(
                    .item(
                        MacMenuItem(
                            verbatim: localizedCoreString(
                                "ui.import.sheet.describes_nothing"
                            ),
                            checked: isBound
                        ) { onBind(nil) }
                    )
                )
            }
        }
        return entries
    }
}
