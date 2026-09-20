import BaeKit
import SwiftUI

/// A track sheet's disc, document, and association summary, followed by one
/// audio assignment control for each FILE reference.
struct ImportSheetCaptionRow: View {
    @Environment(\.sourceFileEditsAllowed)
    private var sourceFileEditsAllowed
    let sheet: BridgeSheetGroup
    /// Identifying signals extracted from this sheet — a cue the disc ID was
    /// computed from. Empty otherwise.
    var evidence: [BridgeFileEvidence]
    /// Whether this surface offers the CUE selection and disc control.
    let showsDiscMenu: Bool
    let actions: ImportMappingActions

    @State
    private var hoveringName = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            caption
            ForEach(sheet.referenceOptions, id: \.fileReference) { reference in
                HStack(spacing: 8) {
                    Text(verbatim: reference.fileReference)
                        .font(.system(size: 11, design: .monospaced))
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text(verbatim: "→")
                        .foregroundStyle(.tertiary)
                    ImportSheetBindingMenu(
                        reference: reference,
                        onBind: {
                            actions.bindSheet(
                                sheet.sheetId,
                                reference.fileReference,
                                $0
                            )
                        }
                    )
                    .disabled(!sourceFileEditsAllowed)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    private var caption: some View {
        HStack(spacing: 8) {
            if showsDiscMenu {
                ImportSheetDiscMenu(
                    sheet: sheet,
                    onAssign: { actions.setSheetDisc(sheet.sheetId, $0) },
                )
                .disabled(!sourceFileEditsAllowed)
            }
            formatTag
            nameButton
            Text(verbatim: "→")
                .font(.system(size: 11))
                .foregroundStyle(.tertiary)
                .fixedSize()
            Text(sheet.bound.descriptionText)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .help(sheet.bound.descriptionText)
            ForEach(ImportEvidence.badges(evidence)) { badge in
                ImportEvidenceChip(signal: badge.signal)
                    .fixedSize()
                    .help(ImportEvidence.hoverText(badge.evidence))
            }
            Spacer(minLength: 0)
        }
    }

    /// What kind of sheet this is. A format name, not a phrase, so it is not
    /// translated.
    private var formatTag: some View {
        Text(verbatim: "CUE")
            .font(.system(size: 9.5, weight: .bold))
            .tracking(0.6)
            .foregroundStyle(Theme.accent)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Theme.accentSoft, in: RoundedRectangle(cornerRadius: 4))
            .fixedSize()
    }

    /// The sheet's name opens it in the viewer.
    private var nameButton: some View {
        Button {
            actions.openDocument(sheet.name, sheet.localPath)
        } label: {
            Text(sheet.name)
                .font(.system(size: 11, design: .monospaced))
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

/// The sheet's disc-assignment control: which of the release's discs its
/// entries are, or that it contributes nothing. A pill, so it reads as a
/// choice already made rather than a field waiting to be filled.
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
                    .font(.system(size: 12, weight: .semibold))
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
            .background(
                Color.primary.opacity(hovering ? 0.13 : 0.09),
                in: RoundedRectangle(cornerRadius: 6)
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
