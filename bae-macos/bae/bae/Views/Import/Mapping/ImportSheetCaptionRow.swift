import BaeKit
import SwiftUI

/// A track sheet's summary, then an audio choice row for each FILE reference
/// with no audio bound; bound references are changed from the summary's menu.
struct ImportSheetCaptionRow: View {
    let sheet: BridgeSheetGroup
    /// Identifying signals extracted from this sheet, such as a Disc ID.
    var evidence: [BridgeFileEvidence]
    /// Whether this surface offers the CUE selection and disc control.
    let showsDiscMenu: Bool
    let actions: ImportMappingActions

    @State
    private var hoveringName = false
    @State
    private var hoveringBound = false

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            caption
            ForEach(layout.unbound, id: \.fileReference) { reference in
                HStack(spacing: ThemeSpace.related) {
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

    private var layout: ImportSheetBindingLayout {
        ImportSheetBindingLayout(sheet: sheet)
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
            Text(verbatim: "→")
                .themeText(.detail)
                .foregroundStyle(.tertiary)
                .fixedSize()
            switch layout.summary {
            case .text:
                boundText
            case .choices(let reference):
                boundMenu {
                    ImportSheetBindingItems(
                        reference: reference,
                        onBind: { bind(reference, to: $0) }
                    )
                }
            case .submenus(let references):
                boundMenu {
                    ForEach(references, id: \.fileReference) { reference in
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

    /// The summary as a menu for changing a bound reference's audio.
    private func boundMenu(
        @ViewBuilder _ items: () -> some View
    ) -> some View {
        Menu {
            items()
        } label: {
            boundText
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .background(
                    hoveringBound ? Theme.hover : Color.clear,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .frame(minWidth: ThemeSize.hitTarget)
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
        StatusChip(verbatim: "CUE", tone: .accent)
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

/// Where a sheet caption offers its FILE references' audio: a reference
/// with none bound is a row of its own, under the summary, with its own menu;
/// bound ones are changed from the summary's menu.
struct ImportSheetBindingLayout: Equatable {
    /// What the summary offers.
    let summary: Summary
    /// The references with no audio bound, in the sheet's order.
    let unbound: [BridgeSheetReferenceOptions]

    enum Summary: Equatable {
        /// Nothing is bound, so the summary is text, not a menu.
        case text
        /// One reference is bound: its choices sit directly in the summary's
        /// menu.
        case choices(BridgeSheetReferenceOptions)
        /// Several are bound: the summary's menu has a submenu per
        /// reference, in the sheet's order.
        case submenus([BridgeSheetReferenceOptions])
    }

    init(sheet: BridgeSheetGroup) {
        let bound = sheet.referenceOptions.filter { $0.fileId != nil }
        unbound = sheet.referenceOptions.filter { $0.fileId == nil }
        if bound.isEmpty {
            summary = .text
        }
        else if bound.count == 1, let only = bound.first {
            summary = .choices(only)
        }
        else {
            summary = .submenus(bound)
        }
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
            HStack(spacing: ThemeSpace.inline) {
                Text(assignmentText)
                    .themeText(.strong)
                Image(systemName: "chevron.down")
                    .themeIcon(.small)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, ThemeSpace.related)
            .padding(.vertical, ThemeSpace.line)
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
