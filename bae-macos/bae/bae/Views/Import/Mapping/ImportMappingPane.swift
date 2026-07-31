import BaeKit
import SwiftUI

/// The one surface a folder becomes a release on: release header, file roles,
/// track slots, commit bar — always in that order, none overlapping.
///
/// It replaces the file pane, the permanently mounted search pane, and the
/// docked confirmation sheet. Search is the header's editor now; the sheet's
/// remaining content is the commit bar.
struct ImportMappingPane: View {
    let candidate: Candidate
    let model: ImportMappingModel
    /// What each track sheet may be bound to, by the sheet's file id.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// The path currently auditioning, if any.
    let previewingPath: String?
    let libraryStatus: BridgeLibraryStatus?
    let hasCoverOptions: Bool
    let coverContent: ImageContent?
    /// The live editor. `nil` while nothing has been picked and no tracklist
    /// has been read off the folder — there is nothing to edit or commit then,
    /// so the slot table and the commit bar stay off the pane.
    let editor: Binding<BridgeRawReleaseEdit>?
    @Binding
    var storageManaged: Bool
    @Binding
    var storagePinned: Bool
    let roleActions: ImportRoleActions
    let slotActions: ImportSlotActions
    let commitActions: ImportCommitActions
    let onFindRelease: () -> Void
    let onEditCover: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    headerZone
                    banners
                    ImportRolesTable(
                        rows: model.roleRows,
                        bindingOptions: bindingOptions,
                        actions: roleActions,
                    )
                    slotsZone
                }
                .padding(20)
            }
            if editor != nil {
                ImportCommitBar(
                    willWriteCount: model.willWriteCount,
                    unansweredCount: model.unansweredCount,
                    candidateKey: candidate.key,
                    importStatus: candidate.importStatus,
                    storageManaged: $storageManaged,
                    storagePinned: $storagePinned,
                    actions: commitActions,
                )
            }
        }
    }

    private var banners: some View {
        ImportConfirmationBanners(
            libraryStatus: libraryStatus,
            importStatus: candidate.importStatus,
            error: candidate.error,
            onViewInLibrary: commitActions.viewInLibrary,
        )
    }
}
