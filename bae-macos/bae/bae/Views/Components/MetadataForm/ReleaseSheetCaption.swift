import BaeKit
import SwiftUI

/// A track sheet's caption in the release editor, in the import pane and in
/// the Edit release sheet alike: its side or disc, its name and the audio it
/// describes. A FILE reference with no audio bound is chosen from the
/// caption's amber menu; bound references are changed from the summary's
/// menu. Where core says the sheet is ignorable, the place pop-up offers
/// *Ignored* and each reference offers binding no audio.
struct ReleaseSheetCaption: View {
    let sheet: BridgeSheetGroup
    let actions: SheetCaptionActions

    @State
    private var hoveringBound = false

    var body: some View {
        MacImportSheetCaption(
            name: sheet.name,
            onOpen: { actions.open(sheet.name, sheet.sheetId) },
            place: {
                SheetPlaceMenu(
                    choices: sheet.sideChoices,
                    assigned: sheet.assigned,
                    offersIgnored: sheet.ignorable,
                    onAssign: { actions.place(sheet.sheetId, $0) }
                )
            },
            audio: { audio }
        )
    }

    private func bind(
        _ reference: BridgeSheetReferenceOptions,
        to fileId: String?
    ) {
        actions.bind(sheet.sheetId, reference.fileReference, fileId)
    }

    private func items(
        _ reference: BridgeSheetReferenceOptions
    ) -> ImportSheetBindingItems {
        ImportSheetBindingItems(
            reference: reference,
            offersNone: sheet.ignorable,
            onBind: { bind(reference, to: $0) }
        )
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
            boundMenu { items(reference) }
        case .several(let references):
            boundMenu {
                for reference in references {
                    MacMenuSubmenu(verbatim: referenceTitle(reference)) {
                        items(reference)
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

    /// A reference as its submenu names it: what the sheet asked for, and
    /// the audio it plays; a reference that plays none is named alone, as
    /// the unbound menu names it.
    private func referenceTitle(
        _ reference: BridgeSheetReferenceOptions
    ) -> String {
        guard let bound = reference.bound else {
            return reference.fileReference
        }
        return String(
            localized: "\(reference.fileReference) → \(bound.name)"
        )
    }
}

/// What a track sheet caption's controls do, as its host runs them.
struct SheetCaptionActions {
    /// Open the sheet in the viewer: its name, then its file id.
    let open: (String, String) -> Void
    /// Put the sheet, by its file id, on a side or disc, or ignore it.
    let place: (String, BridgeSheetPlace) -> Void
    /// Bind one of the sheet's FILE lines to an audio file, or to none: the
    /// sheet's file id, the line's text, then the audio file's id.
    let bind: (String, String, String?) -> Void
}

extension ImportMappingActions {
    /// The import pane's caption actions, which write the candidate's file
    /// decisions.
    var sheetCaption: SheetCaptionActions {
        SheetCaptionActions(
            open: openDocument,
            place: setSheetPlace,
            bind: bindSheet
        )
    }
}
