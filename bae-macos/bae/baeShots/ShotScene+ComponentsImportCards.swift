import BaeKit
import SwiftUI

@testable import bae

// The cards a skipped folder and a library release show in place of an
// import's draft card.

extension ShotScene {
    static let importCardScenes: [ShotScene] =
        invalidCandidateCardScenes + releaseEditorCardScenes

    /// The preview's 720pt box, with the 8pt it leaves round a row.
    private static let cardWidth: CGFloat = 736

    // MARK: Skipped folder

    /// A folder in `2024-rips` whose two audio files are damaged, called
    /// Low Tide Hours by Mara Quill when `tagged`, and nothing otherwise.
    private static func skippedFolder(tagged: Bool) -> BridgeInvalidCandidate {
        let path = Data("/Music/Downloads/2024-rips/untitled 07".utf8)
        return BridgeInvalidCandidate(
            candidateKey: path,
            folderPath: path,
            sourceFolderName: "untitled 07",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            separable: false,
            reason: .damagedAudio(count: 2),
            tags: tagged
                ? BridgeSkippedTags(
                    albumTitle: "Low Tide Hours",
                    artist: .named("Mara Quill")
                ) : nil,
            cover: tagged
                ? .local(file: PreviewData.previewArtFile("Low Tide Hours"))
                : nil
        )
    }

    /// `MacInvalidCandidateCard`: a folder its tags name, with its own
    /// cover, and one they name nothing, by its folder's name.
    private static let invalidCandidateCardScenes: [ShotScene] = [
        component(
            "mac.component.invalid-candidate-card.tagged",
            width: cardWidth,
            height: 240,
            images: PreviewData.artImageStore()
        ) {
            MacInvalidCandidateCard(invalid: skippedFolder(tagged: true))
                .frame(width: 720, alignment: .leading)
        },
        component(
            "mac.component.invalid-candidate-card.no-tags",
            width: cardWidth,
            height: 240,
            images: PreviewData.artImageStore()
        ) {
            MacInvalidCandidateCard(invalid: skippedFolder(tagged: false))
                .frame(width: 720, alignment: .leading)
        },
    ]

    // MARK: Release editor card

    /// Low Tide Hours, a remastered CD from Harbor Records, as the library
    /// holds it.
    private static func editedValues() -> BridgeRawReleaseEdit {
        PreviewData.boardEdit(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            year: "1992",
            pressing: BridgeRawPressingEdit(
                year: "1992",
                labels: [
                    .alone(name: "Harbor Records", catalogNumber: "HR-1042")
                ],
                barcodes: [],
                facts: PreviewData.pressingFacts(
                    country: "US",
                    media: PreviewData.media(.cd),
                    status: .official,
                    discogsDetails: [.remastered]
                )
            ),
            tracks: []
        )
    }

    /// The library album Low Tide Hours by Mara Quill.
    private static let editedAlbumLine = BridgeAlbumLine.titleAndArtists(
        title: "Low Tide Hours",
        artistLine: .named("Mara Quill")
    )

    /// What the editor opens on: the release on the album `album`, its
    /// cover, its files' audio and its MusicBrainz record, with a change
    /// made when `changed`.
    private static func editedRelease(
        album: BridgeDraftAlbum,
        changed: Bool
    ) -> PreviewReleaseEdit {
        PreviewReleaseEdit(
            values: editedValues(),
            draft: BridgeReleaseEditDraft(
                cover: BridgeImageRef(
                    id: "Low Tide Hours",
                    version: "1",
                    imageType: .cover
                ),
                sourceAudio: .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .file,
                        format: PreviewData.previewSourceAudioFormat
                    )
                ),
                sides: nil,
                files: nil,
                album: album,
                records: PreviewData.boardRecords("low-tide-hours"),
                hasRecords: true,
                pressingOffers: PreviewData.pressingOffers,
                albumArtistStanding: PreviewData.boardLibraryArtistStanding,
                changed: changed,
                invalid: nil
            )
        )
    }

    /// The editor card over an editor's session on `release`, ready once
    /// the session's draft arrives; `menu` draws the album pop-up open over
    /// its row.
    private static func releaseEditorCardScene(
        _ id: String,
        _ release: PreviewReleaseEdit,
        menu: Bool = false
    ) -> ShotScene {
        let size = CGSize(width: cardWidth, height: 520)
        return ShotScene(id: id, size: size) {
            let session = PreviewData.releaseEditSession(release: release)
            return component(
                id,
                width: size.width,
                height: size.height,
                images: coverGalleryImages(of: "Low Tide Hours"),
                isReady: { session.draft != nil }
            ) {
                importStores(
                    ShotReleaseEditorCard(session: session, menu: menu)
                )
            }
            .stage()
        }
    }

    /// `MacReleaseEditorCard`: as the editor opens, once something changed,
    /// and with the album pop-up open after the person picked the album.
    /// The component's Reading a file row has no shot: the editor offers no
    /// change that makes a save read a file before it writes.
    private static let releaseEditorCardScenes: [ShotScene] = [
        releaseEditorCardScene(
            "mac.component.release-editor-card.unchanged",
            editedRelease(
                album: .named(
                    albumId: "album-low-tide-hours",
                    line: editedAlbumLine
                ),
                changed: false
            )
        ),
        releaseEditorCardScene(
            "mac.component.release-editor-card.changed",
            editedRelease(
                album: .named(
                    albumId: "album-low-tide-hours",
                    line: editedAlbumLine
                ),
                changed: true
            )
        ),
        releaseEditorCardScene(
            "mac.component.release-editor-card.album-picker-open",
            editedRelease(
                album: .picked(
                    albumId: "album-low-tide-hours",
                    line: editedAlbumLine
                ),
                changed: true
            ),
            menu: true
        ),
    ]
}

/// The editor card over `session`'s draft once it arrives, with the lead
/// the editor gives it; `menu` draws the album pop-up open over its row.
private struct ShotReleaseEditorCard: View {
    let session: ReleaseMetadataEditSession
    let menu: Bool

    var body: some View {
        if let draft = session.draft {
            ZStack(alignment: .topLeading) {
                MacReleaseEditorCard(
                    session: session,
                    draft: draft,
                    onEditCover: {}
                ) {
                    ReleaseEditorLead(
                        session: session,
                        onAutomatic: {},
                        onSearch: {},
                        onCancel: {},
                        onSave: {}
                    )
                }
                .frame(width: 720, alignment: .leading)
                if menu {
                    ShotScene.albumMenu(draft.album, x: 209, y: 160)
                }
            }
        }
    }
}
