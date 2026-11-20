import BaeKit
import SwiftUI

/// The audio choices core offers for the CUE FILE references with no audio
/// bound, as one menu on the sheet's caption: the choices directly for one
/// reference, a submenu per reference for several. Only an import's sheet has
/// such references, so each also offers binding none.
struct ImportSheetUnboundMenu: View {
    let references: [BridgeSheetReferenceOptions]
    let onBind: (BridgeSheetReferenceOptions, String?) -> Void

    @State
    private var hovering = false

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                if let only = references.first, references.count == 1 {
                    ImportSheetBindingItems(
                        reference: only,
                        offersNone: true,
                        onBind: { onBind(only, $0) }
                    )
                }
                else {
                    for reference in references {
                        MacMenuSubmenu(verbatim: reference.fileReference) {
                            ImportSheetBindingItems(
                                reference: reference,
                                offersNone: true,
                                onBind: { onBind(reference, $0) }
                            )
                        }
                    }
                }
            }
        ) {
            Text(localizedCoreString("ui.import.sheet.choose_audio"))
                .themeText(.mono)
                .foregroundStyle(Theme.warning)
                .cutLine(.prose)
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
        .accessibilityLabel(String(localized: "Audio file"))
        .accessibilityValue(
            Text(localizedCoreString("ui.import.sheet.choose_audio"))
        )
    }
}

/// One entry of a FILE reference's binding menu. A reference's entries are
/// each audio file core offers or refuses for it, in core's order, then,
/// where the sheet's tracks can leave the table, binding none.
enum ImportSheetBindingChoice: Hashable {
    /// A file core offers; choosing it binds it to the reference.
    case offered(fileId: String, name: String, isBound: Bool)
    /// A file core refuses, with core's reason. It is listed, dimmed, so the
    /// menu explains itself instead of being empty, and is never chosen.
    case refused(name: String, reason: String)
    /// Binding no audio to the reference.
    case describesNothing(isBound: Bool)

    static func choices(
        for reference: BridgeSheetReferenceOptions,
        offersNone: Bool
    ) -> [Self] {
        let files: [Self] = reference.options.map { option in
            if let reason = option.offer.refusalLine {
                return .refused(name: option.name, reason: reason)
            }
            return .offered(
                fileId: option.fileId,
                name: option.name,
                isBound: reference.fileId == option.fileId
            )
        }
        return offersNone
            ? files + [.describesNothing(isBound: reference.fileId == nil)]
            : files
    }
}

/// The items of one FILE reference's binding menu, one per
/// `ImportSheetBindingChoice`.
struct ImportSheetBindingItems: MacMenuPart {
    let reference: BridgeSheetReferenceOptions
    /// Whether the menu offers binding no audio.
    let offersNone: Bool
    let onBind: (String?) -> Void

    var entries: [MacMenuEntry] {
        var entries: [MacMenuEntry] = []
        let choices = ImportSheetBindingChoice.choices(
            for: reference,
            offersNone: offersNone
        )
        for choice in choices {
            switch choice {
            case .offered(let fileId, let name, let isBound):
                entries.append(
                    .item(
                        MacMenuItem(verbatim: name, checked: isBound) {
                            onBind(fileId)
                        }
                    )
                )
            case .refused(let name, let reason):
                entries.append(
                    .item(
                        MacMenuItem(
                            verbatim: "\(name): \(reason)",
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
