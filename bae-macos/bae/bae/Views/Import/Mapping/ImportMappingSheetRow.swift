import BaeKit
import SwiftUI

/// A track sheet, heading the group of entries it carves: its name, the audio
/// it describes, and which disc of the release those entries are.
///
/// The two controls are the sheet's decisions and nothing else's. Which audio a
/// sheet speaks for is one — a `FILE` directive naming a file that was later
/// re-encoded under another name has no answer but the user's. Which disc it is
/// is the other: cue filenames are arbitrary, `CD1.cue` may hold disc two, so
/// the assignment is the truth and no name is read for it.
struct ImportMappingSheetRow: View {
    let sheet: BridgeSheetGroup
    /// The audio this sheet may be bound to, each already offered or refused by
    /// core. `nil` until it has been asked for; empty means there is nothing to
    /// offer, so no menu appears.
    let options: [BridgeSheetBindingOption]?
    let actions: ImportMappingActions

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: "list.bullet.rectangle")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                Button {
                    actions.openDocument(sheet.name, sheet.localPath)
                } label: {
                    Text(sheet.name)
                        .font(.system(size: 12, design: .monospaced))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .buttonStyle(.plain)
                ImportSheetBindingMenu(
                    sheet: sheet,
                    options: options,
                    onBind: { actions.bindSheet(sheet.sheetId, $0) },
                )
                // Why a sheet is on nothing, where it is on nothing: the
                // directive's own text, or the codec bae cannot carve.
                if let reason = sheet.bound.reasonLine {
                    Text(reason)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 4)
                if let size = sheet.bound.containerSizeText {
                    Text(size)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            ImportSheetDiscMenu(
                sheet: sheet,
                onAssign: { actions.setSheetDisc(sheet.sheetId, $0) },
            )
            .frame(width: ImportMappingColumns.role, alignment: .leading)
            // The becomes half is the entries' to fill; the header only holds
            // its columns open so the group and its rows line up.
            Spacer().frame(width: ImportMappingColumns.position)
            Spacer().frame(maxWidth: .infinity)
            Spacer().frame(maxWidth: .infinity)
            Spacer().frame(width: ImportMappingColumns.trailingColumns)
        }
    }
}

/// The sheet's disc-assignment control: which of the release's discs its
/// entries are, or that it contributes nothing.
struct ImportSheetDiscMenu: View {
    let sheet: BridgeSheetGroup
    let onAssign: (BridgeSheetDisc) -> Void

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
            Text(assignmentText)
                .font(.system(size: 12))
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .foregroundStyle(.secondary)
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
