import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components (`mac.component.<thing>.<state>`), each state a
    /// row of the component's `preview.html`, drawn at the preview's width.
    /// A component with a capture file, `Components/<Component>.swift`,
    /// keeps its scenes there.
    static let componentScenes: [ShotScene] =
        [
            artworkScenes,
            avatarScenes,
            importSheepScenes,
            logoScenes,
            macAccentPickerScenes,
            macActivityScenes,
            macActivityButtonScenes,
            macAlbumArtistFieldScenes,
            macAvatarButtonScenes,
            macAlbumCreditsScenes,
            macAlbumGridScenes,
            macAlbumHeaderScenes,
            macAlbumPanelScenes,
            macAlbumPickerScenes,
            macAlbumRatingScenes,
            macAlbumTileScenes,
            macAlbumWorksScenes,
            macAlertScenes,
            macAnswerActionScenes,
            macArtistAlbumGridScenes,
            macArtistAlbumTileScenes,
            macArtistGroupHeadingScenes,
            macArtistOtherNamesScenes,
            macBackButtonScenes,
            macBoxScenes,
            macBrowseSplitScenes,
            macBulkActionRowScenes,
            macButtonScenes,
            macCastPopoverScenes,
            macCastRowScenes,
            macCheckboxScenes,
            macCloseButtonScenes,
            macCloudHomeScenes,
            macContentUnavailableScenes,
            macCopyTextScenes,
            macCoverGalleryScenes,
            macCoverPreviewScenes,
            macCoverTileScenes,
            macDetailHeaderScenes,
            macDisclosureScenes,
            macErrorDetailsScenes,
            macErrorLineScenes,
            macEyebrowScenes,
            macFactsGridScenes,
            macFailedTransferRowScenes,
            macFieldRowScenes,
            macFileDetailsScenes,
            macFileNameFieldScenes,
            macFileRowScenes,
            macFindOnlineScenes,
            macFolderPickerScenes,
            macFormFieldScenes,
            macFormHeadingScenes,
            macHairlineScenes,
            macIconButtonScenes,
            macIdentifierBandScenes,
            macIdentifierChipScenes,
            macIdentifyBarScenes,
            macImportBulkScenes,
            macImportDestinationScenes,
            macImportEmptyStateScenes,
            macImportFilterRowScenes,
            macImportFolderLineScenes,
            macImportGroupHeaderScenes,
            macImportImagesScenes,
            macImportReleaseSummaryScenes,
            macImportRowScenes,
            macImportSheetCaptionScenes,
            macImportSidebarScenes,
            macImportTableScenes,
            macImportTabsScenes,
            macImportTrackRowScenes,
            macImportWindowScenes,
            macInfoTipScenes,
            macInlineFieldScenes,
            macInvalidCandidateCardScenes,
            macInviteFieldsScenes,
            macItemScenes,
            macLibraryHeadingScenes,
            macLibraryRowScenes,
            macLightboxScenes,
            macListRowScenes,
            macMainWindowScenes,
            macMatchCodeScenes,
            macMenuScenes,
            macMenuItemScenes,
            macModalScenes,
            macModalHeaderScenes,
            macMoreMenuScenes,
            macNestedScenes,
            macNoteScenes,
            macNoticeScenes,
            macNowPlayingBarScenes,
            macPageHeadingScenes,
            macPersonTagScenes,
            macPinFieldScenes,
            macPlaceholderLineScenes,
            macPlayPauseScenes,
            macPlaylistHeaderScenes,
            macPlaylistPageScenes,
            macPopUpButtonScenes,
            macPopoverScenes,
            macPortFieldScenes,
            macPressingRowScenes,
            macProgressScenes,
            macProgressLineScenes,
            macProgressRingScenes,
            macQRScannerScenes,
            macQueueInsertLineScenes,
            macQueuePanelScenes,
            macQueuePlaceholderRowScenes,
            macQueueRateScenes,
            macQueueRowScenes,
            macQueueSectionScenes,
            macRadioScenes,
            macRecordLinksScenes,
            macReleaseCardScenes,
            macReleaseCellScenes,
            macReleaseEditorScenes,
            macReleaseFieldsScenes,
            macReleaseFileRowScenes,
            macReleaseFileTableScenes,
            macReleaseFilesScenes,
            macReleaseGroupScenes,
            macReleaseIdentityScenes,
            macReleasePickerScenes,
            macRestoreCodeScenes,
            macRowScenes,
            macS3FormScenes,
            macS3KeyFieldsScenes,
            macSavePanelFormatScenes,
            macScanCodeScenes,
            macSearchCardScenes,
            macSearchFieldScenes,
            macSearchSectionScenes,
            macSearchSourceLineScenes,
            macSectionScenes,
            macSectionHeaderScenes,
            macSectionTabsScenes,
            macSeekBarScenes,
            macSegmentedControlScenes,
            macSettingsSidebarScenes,
            macSettingsWindowScenes,
            macSheetScenes,
            macSheetFooterScenes,
            macSidePauseScenes,
            macSortCriteriaScenes,
            macSortPillScenes,
            macSourcedValueScenes,
            macSpinnerScenes,
            macStepScenes,
            macStepActionsScenes,
            macStepperScenes,
            macStorageBandScenes,
            macStorageCellScenes,
            macStorageChipScenes,
            macStorageFooterScenes,
            macStorageStateScenes,
            macStorageToolbarScenes,
            macStorageUsageScenes,
            macStorageWindowScenes,
            macSidePaneHeaderScenes,
            macSwitchScenes,
            macSyncStatusScenes,
            macTableScenes,
            macTextFieldScenes,
            macThumbnailStripScenes,
            macTitleBarScenes,
            macTokenScenes,
            macTrackColumnsScenes,
            macTrackLeadScenes,
            macTrackRowScenes,
            macTransferLineScenes,
            macTransferReadoutScenes,
            macTransferSummaryScenes,
            macTransportScenes,
            macUsageKeyScenes,
            macVolumeSliderScenes,
            macWelcomeActionsScenes,
            macWelcomeHomeScenes,
            macWelcomeLibrariesScenes,
            macWelcomeWindowScenes,
            macWorkDetailScenes,
            playlistPictureScenes,
            starsScenes,
            statusChipScenes,
            statusGlyphScenes,
        ]
        .flatMap { $0 }

    /// A component's state: `view` in a column `width` points wide, over
    /// the elevated surface the preview draws every component on. A state's
    /// board is its preview row's cell, which holds the component at its top
    /// left with no margin. A `page` state is a preview with no rows, whose
    /// board is the whole page: the page sets the component in by its edge
    /// space, so the frame does too. `height` is room for the state; the
    /// picture's comparison ends at the board's height.
    static func component<Content: View>(
        _ id: String,
        page: Bool = false,
        width: CGFloat = 380,
        height: CGFloat,
        images: ImageStore? = nil,
        isReady: @escaping @MainActor () -> Bool = { true },
        @ViewBuilder view: @escaping @MainActor () -> Content
    ) -> ShotScene {
        let margin = page ? ThemeSpace.edge : 0
        let size = CGSize(
            width: width + 2 * margin,
            height: height + 2 * margin
        )
        return ShotScene(id: id, size: size, frame: .component) {
            let content = view()
                .padding(margin)
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .topLeading
                )
                .background(Theme.surfaceElevated)
            // Every view that draws art reads the store from the
            // environment, as the app's window passes it.
            guard let images else {
                return ShotStage(
                    content.environment(ImageStore.stub()),
                    isReady: isReady
                )
            }
            return ShotStage(
                content.environment(images),
                images: images,
                isReady: isReady
            )
        }
    }
}

/// A `MacBox`: the box of rows the settings panes draw, as the design's
/// preview shows a row among others. A box outside a form is a `MacBox`,
/// the form style's own rows and surface without its page margin.
struct ShotBox<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        MacBox { content }
    }
}

/// A settings form drawn in a component's frame: the form style's sections
/// without the page margin a pane puts round them.
struct ShotForm<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        Form { content }
            .formStyle(.settingsWithoutMargin)
            .scrollDisabled(true)
    }
}
