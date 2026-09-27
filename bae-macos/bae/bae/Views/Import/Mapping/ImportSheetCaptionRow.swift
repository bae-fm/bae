import BaeKit
import SwiftUI

/// A track sheet's summary, then an audio choice row for each FILE reference
/// with no audio bound; bound references are changed from the summary's menu.
struct ImportSheetCaptionRow: View {
    let sheet: BridgeSheetGroup
    /// Identifying signals extracted from this sheet, such as a disc ID.
    var evidence: [BridgeFileEvidence]
    /// Whether this surface offers the CUE selection and disc control.
    let showsDiscMenu: Bool
    let actions: ImportMappingActions

    @State
    private var hoveringName = false
    @State
    private var hoveringBound = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            caption
            ForEach(unbound, id: \.fileReference) { reference in
                HStack(spacing: 8) {
                    Text(verbatim: reference.fileReference)
                        .themeText(.mono)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(verbatim: "→")
                        .foregroundStyle(.tertiary)
                    ImportSheetBindingMenu(
                        reference: reference,
                        onBind: { bind(reference, to: $0) }
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    /// The references with no audio bound, in the sheet's order.
    private var unbound: [BridgeSheetReferenceOptions] {
        sheet.referenceOptions.filter { $0.fileId == nil }
    }

    /// The references with audio bound, in the sheet's order.
    private var bound: [BridgeSheetReferenceOptions] {
        sheet.referenceOptions.filter { $0.fileId != nil }
    }

    private func bind(
        _ reference: BridgeSheetReferenceOptions,
        to fileId: String?
    ) {
        actions.bindSheet(sheet.sheetId, reference.fileReference, fileId)
    }

    private var caption: some View {
        HStack(spacing: 8) {
            if showsDiscMenu {
                ImportSheetDiscMenu(
                    sheet: sheet,
                    onAssign: { actions.setSheetDisc(sheet.sheetId, $0) },
                )
            }
            formatTag
            nameButton
            Text(verbatim: "→")
                .themeText(.detail)
                .foregroundStyle(.tertiary)
                .fixedSize()
            if bound.isEmpty {
                boundText
            }
            else {
                boundMenu
            }
            ForEach(ImportEvidence.badges(evidence)) { badge in
                ImportEvidenceChip(signal: badge.signal)
                    .fixedSize()
                    .help(ImportEvidence.hoverText(badge.evidence))
            }
            Spacer(minLength: 0)
        }
    }

    private var boundText: some View {
        Text(sheet.bound.descriptionText)
            .themeText(.mono)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .truncationMode(.middle)
            .help(sheet.bound.descriptionText)
    }

    /// The summary as a menu, where a bound reference's audio is changed: one
    /// reference's choices directly, or a submenu per reference when the
    /// sheet names several files.
    private var boundMenu: some View {
        Menu {
            if bound.count == 1, let only = bound.first {
                ImportSheetBindingItems(
                    reference: only,
                    onBind: { bind(only, to: $0) }
                )
            }
            else {
                ForEach(bound, id: \.fileReference) { reference in
                    Menu {
                        ImportSheetBindingItems(
                            reference: reference,
                            onBind: { bind(reference, to: $0) }
                        )
                    } label: {
                        Text(verbatim: referenceTitle(reference))
                    }
                }
            }
        } label: {
            boundText
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(
                    hoveringBound ? Theme.hover : Color.clear,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .frame(minWidth: 24)
        .onHover { hoveringBound = $0 }
    }

    /// A bound reference as its submenu names it: what the sheet asked for,
    /// and the audio it has.
    private func referenceTitle(
        _ reference: BridgeSheetReferenceOptions
    ) -> String {
        "\(reference.fileReference) → \(reference.fileId ?? "")"
    }

    /// What kind of sheet this is; a format name, so it is not translated.
    private var formatTag: some View {
        Text(verbatim: "CUE")
            .themeText(.chip)
            .foregroundStyle(Theme.accent)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                Theme.accentSoft,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
            .fixedSize()
    }

    /// The sheet's name opens it in the viewer.
    private var nameButton: some View {
        Button {
            actions.openDocument(sheet.name, sheet.localPath)
        } label: {
            Text(sheet.name)
                .themeText(.mono)
                .foregroundStyle(hoveringName ? .primary : .secondary)
                .underline(hoveringName)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .buttonStyle(.plain)
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
        Menu {
            ForEach(sheet.discOptions, id: \.self) { number in
                Button {
                    onAssign(.disc(number: number))
                } label: {
                    checkable(
                        coreString("ui.import.sheet.disc", Int(number)),
                        selected: sheet.assignment == .disc(number: number)
                    )
                }
            }
            Divider()
            Button {
                onAssign(.ignored)
            } label: {
                checkable(
                    coreString("ui.import.sheet.ignored"),
                    selected: sheet.assignment == .ignored
                )
            }
        } label: {
            HStack(spacing: 5) {
                Text(assignmentText)
                    .themeText(.strong)
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
            .background(
                hovering ? Theme.pressed : Theme.hover,
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .onHover { hovering = $0 }
        .help(coreString("ui.import.sheet.disc_help"))
    }

    private var assignmentText: String {
        switch sheet.assignment {
        case .disc(let number):
            coreString("ui.import.sheet.disc", Int(number))
        case .ignored:
            coreString("ui.import.sheet.ignored")
        }
    }

    @ViewBuilder
    private func checkable(_ label: String, selected: Bool) -> some View {
        if selected {
            Label(label, systemImage: "checkmark")
        }
        else {
            Text(label)
        }
    }
}
