import BaeKit
import SwiftUI

/// A track sheet's caption: its disc, its name and the audio it describes.
/// A FILE reference with no audio bound is chosen from the caption's amber
/// menu; bound references are changed from the summary's menu.
struct MacImportSheetCaption: View {
    let sheet: BridgeSheetGroup
    /// Whether this surface offers the CUE selection and disc control.
    let showsDiscMenu: Bool
    let actions: ImportMappingActions

    @State
    private var hoveringName = false
    @State
    private var hoveringBound = false

    var body: some View {
        caption
    }

    private func bind(
        _ reference: BridgeSheetReferenceOptions,
        to fileId: String?
    ) {
        actions.bindSheet(sheet.sheetId, reference.fileReference, fileId)
    }

    private var caption: some View {
        HStack(spacing: ThemeSpace.related) {
            if showsDiscMenu {
                ImportSheetDiscMenu(
                    sheet: sheet,
                    onAssign: { actions.setSheetDisc(sheet.sheetId, $0) },
                )
            }
            formatTag
            nameButton
            // The arrow joins the sheet to its audio for the eye; the
            // pop-up after it says what it is.
            Image(systemName: "arrow.forward")
                .themeIcon(.small)
                .foregroundStyle(.tertiary)
                .fixedSize()
                .accessibilityHidden(true)
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
            Spacer(minLength: 0)
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

    /// What kind of sheet this is; a format name, so it is not translated.
    private var formatTag: some View {
        StatusChip(verbatim: "CUE", tone: .accent)
            .fixedSize()
    }

    /// The sheet's name opens it in the viewer.
    private var nameButton: some View {
        Button {
            actions.openDocument(sheet.name, sheet.sheetId)
        } label: {
            Text(sheet.name)
                .themeText(.mono)
                .foregroundStyle(.primary)
                .underline(hoveringName)
                .cutLine(.fileName)
        }
        .buttonStyle(.accentPlain)
        .layoutPriority(1)
        .onHover { hoveringName = $0 }
    }
}

/// The sheet's disc assignment: which of the release's discs its entries are,
/// or that it is ignored.
struct ImportSheetDiscMenu: View {
    let sheet: BridgeSheetGroup
    let onAssign: (BridgeSheetDisc) -> Void

    @State
    private var hovering = false

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                for number in sheet.discOptions {
                    MacMenuItem(
                        verbatim: localizedCoreString(
                            "ui.import.sheet.disc",
                            Int(number)
                        ),
                        checked: sheet.assignment == .disc(number: number)
                    ) { onAssign(.disc(number: number)) }
                }
                MacMenuItem.divider
                MacMenuItem(
                    verbatim: localizedCoreString("ui.import.sheet.ignored"),
                    checked: sheet.assignment == .ignored
                ) { onAssign(.ignored) }
            }
        ) {
            HStack(spacing: ThemeSpace.inline) {
                Text(assignmentText)
                    .themeText(.strong)
                Image(systemName: "chevron.down")
                    .themeIcon(.small)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background(
                hovering ? Theme.pressed : Theme.hover,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
        }
        .fixedSize()
        .onHover { hovering = $0 }
        .help(localizedCoreString("ui.import.sheet.disc_help"))
        .accessibilityLabel(String(localized: "Disc"))
        .accessibilityValue(Text(verbatim: assignmentText))
    }

    private var assignmentText: String {
        switch sheet.assignment {
        case .disc(let number):
            localizedCoreString("ui.import.sheet.disc", Int(number))
        case .ignored:
            localizedCoreString("ui.import.sheet.ignored")
        }
    }
}
