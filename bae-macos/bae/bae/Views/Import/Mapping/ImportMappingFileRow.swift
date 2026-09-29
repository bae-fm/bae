import BaeKit
import SwiftUI

/// One file carried with the release that is not one of its tracks: a rip log,
/// a text file, a checksum list.
///
/// It says its name and size. Nothing says it is kept — it is listed in the
/// import under a heading that says Files, which is the same statement without
/// the sentence.
struct ImportMappingFileRow: View {
    let file: BridgeMappingFile
    let previewingTarget: BridgePreviewTarget?
    /// Identifying signals extracted from this file — the rip log a disc ID
    /// was computed from wears its chip here.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions

    var body: some View {
        ImportMappingSourceCell(
            source: .file(file: file),
            previewingTarget: previewingTarget,
            evidence: evidence,
            showsFileSize: true,
            actions: actions,
        )
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
