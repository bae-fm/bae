import BaeKit
import SwiftUI

/// The mapping pane's foot: where the files go, what will be written, what
/// is still unanswered, and the button.
///
/// Nothing here disables the commit. The counts are statements — a folder with
/// a row nobody named still imports, and the one refusal left in the whole
/// import is audio that will not decode, which core raises.
struct ImportCommitBar: View {
    let willWriteCount: Int
    let unansweredCount: Int
    /// Routes the loudness ticks to the leaf progress bar during the loudness
    /// pass.
    let candidateKey: String
    let importStatus: BridgeCandidateImportStatus?
    @Binding
    var storageManaged: Bool
    @Binding
    var storagePinned: Bool
    let actions: ImportCommitActions

    @Environment(ConfigStore.self)
    private var configStore

    private var settled: Bool {
        guard let importStatus else { return false }
        switch importStatus {
        case .importing, .complete: return true
        case .error: return false
        }
    }

    var body: some View {
        HStack(alignment: .center, spacing: 16) {
            counts
            Spacer(minLength: 12)
            if !settled, configStore.config.hasCloudHome {
                storageToggles
            }
            ImportConfirmationCardAction(
                importStatus: importStatus,
                candidateKey: candidateKey,
                onConfirmImport: actions.confirmImport,
                onViewInLibrary: actions.viewInLibrary,
            )
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .background(Theme.surface)
        .overlay(alignment: .top) {
            Rectangle().fill(.white.opacity(0.1)).frame(height: 1)
        }
    }

    private var counts: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(coreString("ui.import.commit.will_write", willWriteCount))
                .font(.system(size: 12.5))
                .foregroundStyle(.secondary)
            if unansweredCount > 0 {
                Text(coreString("ui.import.commit.unanswered", unansweredCount))
                    .font(.system(size: 11.5))
                    .foregroundStyle(.orange)
            }
        }
    }

    private var storageToggles: some View {
        HStack(spacing: 10) {
            ImportCheckboxToggle("Managed", isOn: $storageManaged)
            if storageManaged {
                ImportCheckboxToggle("Keep local copy", isOn: $storagePinned)
            }
        }
        .fixedSize()
    }
}
