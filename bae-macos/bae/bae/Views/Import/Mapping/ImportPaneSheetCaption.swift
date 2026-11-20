import BaeKit
import SwiftUI

/// A track sheet's caption: its disc, its name and the audio it describes.
/// A FILE reference with no audio bound is chosen from the caption's amber
/// menu; bound references are changed from the summary's menu.
struct ImportPaneSheetCaption: View {
    let sheet: BridgeSheetGroup
    /// Whether this surface offers the CUE selection and disc control.
    let showsDiscMenu: Bool
    let actions: ImportMappingActions

    @State
    private var hoveringBound = false

    var body: some View {
        MacImportSheetCaption(
            name: sheet.name,
            onOpen: { actions.openDocument(sheet.name, sheet.sheetId) },
            disc: {
                if showsDiscMenu {
                    SheetDiscMenu(
                        discs: sheet.discOptions,
                        assignment: sheet.assignment,
                        offersIgnored: true,
                        onAssign: { actions.setSheetDisc(sheet.sheetId, $0) }
                    )
                }
            },
            audio: { audio }
        )
    }

    private func bind(
        _ reference: BridgeSheetReferenceOptions,
        to fileId: String?
    ) {
        actions.bindSheet(sheet.sheetId, reference.fileReference, fileId)
    }

    /// The audio the sheet's FILE lines are bound to, and the pop-up that
    /// binds the lines that have none.
    @ViewBuilder
    private var audio: some View {
        switch sheet.boundAudio {
        case .none:
            if sheet.unboundReferences == nil {
                boundText
            }
        case .one(let reference):
            boundMenu {
                ImportSheetBindingItems(
                    reference: reference,
                    onBind: { bind(reference, to: $0) }
                )
            }
        case .several(let references):
            boundMenu {
                for reference in references {
                    MacMenuSubmenu(verbatim: referenceTitle(reference)) {
                        ImportSheetBindingItems(
                            reference: reference,
                            onBind: { bind(reference, to: $0) }
                        )
                    }
                }
            }
        }
        if let unbound = sheet.unboundReferences {
            ImportSheetUnboundMenu(
                references: unbound,
                onBind: { bind($0, to: $1) }
            )
        }
    }

    private var boundText: some View {
        Text(sheet.bound.descriptionText)
            .themeText(.mono)
            .foregroundStyle(.secondary)
            .cutLine(sheet.bound.lineCut)
            .help(sheet.bound.descriptionText)
    }

    /// The summary as a menu for changing a bound reference's audio.
    private func boundMenu(
        @MacMenuBuilder _ items: @escaping () -> [MacMenuEntry]
    ) -> some View {
        MacButton(kind: .plain, menu: items) {
            boundText
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .background(
                    hoveringBound ? Theme.hover : Color.clear,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
                .frame(minWidth: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .onHover { hoveringBound = $0 }
        .accessibilityLabel(String(localized: "Audio file"))
        .accessibilityValue(Text(verbatim: sheet.bound.descriptionText))
    }

    /// A bound reference as its submenu names it: what the sheet asked for,
    /// and the audio it has.
    private func referenceTitle(
        _ reference: BridgeSheetReferenceOptions
    ) -> String {
        String(
            localized: "\(reference.fileReference) → \(reference.fileId ?? "")"
        )
    }
}
