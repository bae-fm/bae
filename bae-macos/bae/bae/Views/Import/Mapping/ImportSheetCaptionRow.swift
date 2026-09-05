import BaeKit
import SwiftUI

/// A track sheet's caption over the rows it carves: which disc of the release
/// those entries are, the sheet's name, and the audio it describes — read left
/// to right as one line, outside the table's columns.
///
/// The two controls are the sheet's decisions and nothing else's. Which audio a
/// sheet speaks for is one — a `FILE` directive naming a file that was later
/// re-encoded under another name has no answer but the user's. Which disc it is
/// is the other: cue filenames are arbitrary, `CD1.cue` may hold disc two, so
/// the assignment is the truth and no name is read for it.
struct ImportSheetCaptionRow: View {
    let sheet: BridgeSheetGroup
    /// The audio this sheet may be bound to, each already offered or refused by
    /// core. `nil` until it has been asked for; empty means there is nothing to
    /// offer, so no menu appears.
    let options: [BridgeSheetBindingOption]?
    /// Identifying signals extracted from this sheet — a cue the disc ID was
    /// computed from. Empty otherwise.
    var evidence: [BridgeFileEvidence]
    /// Whether the disc menu is on the line. An import with one sheet has one
    /// disc and the pill would only restate it; a sheet taken out of the
    /// tracklist keeps the menu whatever the count, because it is the way
    /// back in.
    let showsDiscMenu: Bool
    let actions: ImportMappingActions

    @State
    private var hoveringName = false

    var body: some View {
        HStack(spacing: 8) {
            if showsDiscMenu {
                ImportSheetDiscMenu(
                    sheet: sheet,
                    onAssign: { actions.setSheetDisc(sheet.sheetId, $0) },
                )
            }
            formatTag
            nameButton
            if hasBinding {
                Text(verbatim: "\u{2192}")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                    .fixedSize()
                binding
            }
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

    /// There is a binding to show when core has offered audio to bind to, the
    /// sheet is already on one file, or core has a refusal to explain.
    private var hasBinding: Bool {
        options?.isEmpty == false
            || sheet.bound.containerName != nil
            || sheet.bound.reasonLine != nil
    }

    /// The audio the sheet describes: the menu that chooses it where there is
    /// a choice, else the name alone.
    @ViewBuilder
    private var binding: some View {
        if let options, !options.isEmpty {
            ImportSheetBindingMenu(
                sheet: sheet,
                options: options,
                onBind: { actions.bindSheet(sheet.sheetId, $0) },
            )
        }
        else if let name = sheet.bound.containerName {
            Text(name)
                .font(.system(size: 11, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.middle)
        }
        else if let reason = sheet.bound.reasonLine {
            Text(reason)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
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
