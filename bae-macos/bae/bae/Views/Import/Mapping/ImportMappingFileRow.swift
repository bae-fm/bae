import BaeKit
import SwiftUI

/// One file carried with the release that is not one of its tracks: a rip log,
/// a text file, audio somebody took out of the tracklist.
///
/// It says its name and size. Nothing says it is kept — it is listed in the
/// import under a heading that says Files, which is the same statement without
/// the sentence. An excluded audio file also keeps the action that puts it back
/// in the track list.
struct ImportMappingFileRow: View {
    let file: BridgeMappingFile
    let columns: ImportMappingColumns.Files
    let previewingTarget: BridgePreviewTarget?
    /// Identifying signals extracted from this file — the rip log a disc ID
    /// was computed from wears its chip here.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            HStack(spacing: ImportMappingColumns.spacing) {
                ImportMappingSourceCell(
                    source: .file(file: file),
                    previewingTarget: previewingTarget,
                    evidence: evidence,
                    showsFileSize: false,
                    actions: actions,
                )
                .frame(maxWidth: .infinity, alignment: .leading)
                roleControl
            }
            .frame(width: columns.name, alignment: .leading)
            Text(sizeText)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: columns.size, alignment: .trailing)
        }
    }

    @ViewBuilder
    private var roleControl: some View {
        if !file.alternatives.isEmpty {
            ImportRoleChoiceControl(
                alternatives: file.alternatives,
                inForce: file.roleChoice,
                onPick: { actions.setRole(file.fileId, $0) },
            )
        }
    }

    private var sizeText: String {
        return file.sizeText
    }
}
