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
    @Environment(\.sourceFileEditsAllowed)
    private var sourceFileEditsAllowed
    let file: BridgeMappingFile
    let previewingTarget: BridgePreviewTarget?
    /// Identifying signals extracted from this file — the rip log a disc ID
    /// was computed from wears its chip here.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            ImportMappingSourceCell(
                source: .file(file: file),
                previewingTarget: previewingTarget,
                evidence: evidence,
                showsFileSize: true,
                actions: actions,
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            roleControl
                .disabled(!sourceFileEditsAllowed)
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
}
