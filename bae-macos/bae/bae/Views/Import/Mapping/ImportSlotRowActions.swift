import BaeKit
import SwiftUI

/// The slot row's actions: pick the audio this row writes, and the one action
/// that belongs to the row's own disagreement — Exclude for audio the source
/// does not name, Drop for a track this folder has nothing for.
///
/// Re-pairing is this menu, not a drag. A drag needs a second hit target and a
/// second interaction design per toolkit, has no keyboard or accessibility
/// path, and buys nothing over picking from the folder's audio by name — which
/// is what re-pointing a row and swapping two rows both come down to.
struct ImportSlotRowActions: View {
    let row: ImportSlotRow
    let audioChoices: [BridgeSlotFile]
    let actions: ImportSlotActions

    var body: some View {
        HStack(spacing: 8) {
            chooseFileMenu
            if row.position == nil, let file = row.file {
                Button(coreString("ui.import.slots.exclude")) {
                    actions.exclude(file.audio.fileId)
                }
                .buttonStyle(.link)
                .font(.system(size: 11.5))
            }
            else if row.file == nil {
                Button(coreString("ui.import.slots.drop")) {
                    actions.drop(row.index)
                }
                .buttonStyle(.link)
                .font(.system(size: 11.5))
            }
        }
    }

    /// Every audio unit the folder offers, by name and size. Picking one writes
    /// it onto this row, which is what pairs a slot the source named with
    /// nothing behind it — and what re-points a row already paired.
    @ViewBuilder
    private var chooseFileMenu: some View {
        if !audioChoices.isEmpty {
            Menu {
                ForEach(audioChoices, id: \.audio) { choice in
                    Button {
                        actions.chooseFile(row.index, choice.audio)
                    } label: {
                        Text(verbatim: "\(choice.name) — \(choice.sizeText)")
                    }
                }
            } label: {
                Text(coreString("ui.import.slots.choose_file"))
                    .font(.system(size: 11.5))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .foregroundStyle(.secondary)
        }
    }
}
