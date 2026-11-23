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
            }
        ) {
            Text(localizedCoreString("ui.import.sheet.choose_audio"))
                .themeText(.mono)
                .foregroundStyle(Theme.warning)
                .cutLine(.prose)
                .hoverPlate(hovering)
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
        for reference: BridgeSheetReferenceOptions
    ) -> [Self] {
        let files: [Self] = reference.options.map { option in
            if let reason = option.offer.refusalLine {
                return .refused(name: option.name, reason: reason)
            }
            return .offered(
                fileId: option.fileId,
                name: option.name,
                isBound: reference.bound?.fileId == option.fileId
            )
        }
        return files + [.describesNothing(isBound: reference.bound == nil)]
    }
}

/// The items of one FILE reference's binding menu, one per
/// `ImportSheetBindingChoice`.
struct ImportSheetBindingItems: MacMenuPart {
    let reference: BridgeSheetReferenceOptions
    let onBind: (String?) -> Void

    var entries: [MacMenuEntry] {
        var entries: [MacMenuEntry] = []
        let choices = ImportSheetBindingChoice.choices(for: reference)
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

extension View {
    /// The hover fill of a menu button that is only its words: it reaches past
    /// the words without moving them, so the caption lays out as its words.
    func hoverPlate(_ hovering: Bool) -> some View {
        background {
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(hovering ? Theme.hover : Color.clear)
                .padding(.horizontal, -ThemeSpace.compact)
                .padding(.vertical, -ThemeSpace.line)
        }
    }
}
