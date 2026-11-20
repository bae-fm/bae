import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The basic Mac components, the pictures and chips both platforms
    /// share, and the dialogs: notes, items, menus and pop-ups, alerts and
    /// sheets, covers, avatars, the logo, stars and chips.
    static let basicsComponentScenes: [ShotScene] =
        noteScenes + itemAmongRowsScenes + moreMenuOpenScenes
        + popUpButtonScenes
        + matchCodeWaitingScenes + dialogScenes + backButtonScenes
        + queueInsertLineScenes + artworkScenes
        + avatarScenes + logoScenes + playlistPictureScenes + starsScenes
        + statusChipScenes

    /// `MacNote`, a loose paragraph between boxes.
    private static let noteScenes: [ShotScene] = [
        component("mac.component.note.default", page: true, height: 72) {
            MacNote(
                "A shared folder is required to invite people to the library."
            )
            .frame(width: 380, alignment: .leading)
        }
    ]

    /// `MacItem` among a list's rows, with a failed transfer's notice between
    /// two files.
    private static let itemAmongRowsScenes: [ShotScene] = [
        component("mac.component.item.among-rows-with-a-notice", height: 210) {
            let op = BridgeDownloadOp(
                releaseId: "rel-glass-harbor",
                kind: .pin,
                fileCount: 9,
                totalSize: 356_000_000,
                release: BridgeQueuedRelease(title: "Glass Harbor"),
                createdAt: 0,
                state: .failed(error: "The network connection was lost.")
            )
            VStack(spacing: 0) {
                itemFile("01 Harbor Lights.flac", size: 34_100_000)
                MacFailedTransferRow(
                    entry: .download(op, onRetry: {}),
                    onCancel: {}
                )
                itemFile("02 The Long Way Home.flac", size: 38_900_000)
            }
            .frame(width: 380, alignment: .leading)
        }
    ]

    /// A stored audio file's row.
    private static func itemFile(_ name: String, size: UInt64) -> MacFileRow {
        MacFileRow(
            row: BridgeStorageInspectorFile(
                file: BridgeFile(
                    id: name,
                    originalFilename: name,
                    name: name,
                    folder: nil,
                    kind: .audio(format: "FLAC"),
                    fileSize: size,
                    contentType: "application/octet-stream",
                    isImage: false,
                    audioFormat: nil
                ),
                upload: nil,
                pin: nil,
                problem: nil
            )
        )
    }

    /// `MacMoreMenu` open: its menu drawn in place 4 points under the button,
    /// the item the pointer is on highlighted.
    private static let moreMenuOpenScenes: [ShotScene] = [
        component("mac.component.more-menu.open", height: 150) {
            ZStack(alignment: .topTrailing) {
                MacMoreMenu {
                    MacMenuItem("Details…") {}
                }
                MacMenuInPlace(
                    {
                        MacMenuItem("Details…") {}
                        MacMenuItem("Rename…") {}
                        MacMenuItem.divider
                        MacMenuItem("Remove from this Mac…") {}
                    },
                    highlighted: 1
                )
                .padding(.top, ThemeSize.hitTarget + 4)
            }
            .frame(width: 380, alignment: .topTrailing)
        }
    ]

    /// `MacPopUpButton` at rest, off, and open: the menu drawn in place over
    /// the button, the chosen item on the button's line.
    private static let popUpButtonScenes: [ShotScene] = [
        component("mac.component.pop-up-button.resting", height: 50) {
            popUp()
        },
        component("mac.component.pop-up-button.off", height: 50) {
            popUp().disabled(true)
        },
        component("mac.component.pop-up-button.open", height: 130) {
            let options: [MacPopUpEntry<String>] = [
                .option(MacPopUpOption(verbatim: "Off", value: "Off")),
                .option(MacPopUpOption(verbatim: "Only me", value: "Only me")),
                .option(
                    MacPopUpOption(verbatim: "Everyone", value: "Everyone")
                ),
            ]
            let menu = MacMenu(options, selection: "Only me", select: { _ in })
            ZStack(alignment: .topLeading) {
                popUp()
                    .padding(.leading, 40)
                    .padding(.top, 40)
                MacMenuInPlace(
                    { menu.entries },
                    highlighted: menu.chosen,
                    isPopUp: true
                )
                // The menu's left edge is 31 points left of the button's,
                // and its chosen line is on the button's.
                .padding(.leading, 40 - 31)
                .padding(.top, 40 - 6 - 24)
            }
        },
    ]

    private static func popUp() -> some View {
        MacPopUpButton("Share with", selection: .constant("Only me")) {
            MacPopUpOption(verbatim: "Off", value: "Off")
            MacPopUpOption(verbatim: "Only me", value: "Only me")
            MacPopUpOption(verbatim: "Everyone", value: "Everyone")
        }
    }

    /// `MacMatchCode` with the line that says what this device waits for.
    private static let matchCodeWaitingScenes: [ShotScene] = [
        component("mac.component.match-code.waiting", height: 110) {
            MacMatchCode(
                code: "412 778",
                waiting: "Waiting for your other device…"
            )
            .frame(width: 380)
        }
    ]

    /// `MacAlert` and `MacSheet`, each hung from the top of a window
    /// background as the design's previews draw them, and `MacModal` over
    /// the page.
    private static let dialogScenes: [ShotScene] = [
        component(
            "mac.component.alert.default",
            width: 600,
            height: 764
        ) {
            VStack(spacing: ThemeSpace.group) {
                alertOver(
                    height: 240,
                    MacAlertWords(
                        Text(verbatim: "Remove library from this Mac?"),
                        message: Text(
                            verbatim:
                                "The library will remain in the cloud and on your other devices."
                        )
                    ),
                    [
                        MacAlertButton(
                            Text(verbatim: "Remove"),
                            role: .destructive
                        ),
                        MacAlertButton(Text(verbatim: "Cancel"), role: .cancel),
                    ]
                )
                alertOver(
                    height: 240,
                    MacAlertWords(
                        Text(verbatim: "Could not remove Home music"),
                        message: Text(
                            verbatim:
                                "Could not reach the cloud. Check your connection and try again."
                        )
                    ),
                    [
                        MacAlertButton(Text(verbatim: "Retry")),
                        MacAlertButton(Text(verbatim: "Cancel"), role: .cancel),
                    ]
                )
                alertOver(
                    height: 260,
                    MacAlertWords(
                        Text(verbatim: "Unpin Low Tide Hours?"),
                        message: Text(
                            verbatim:
                                "Its files will be removed from this Mac now. It stays in the cloud."
                        ),
                        suppression: .init(
                            Text(verbatim: "Do not show this again"),
                            isOn: .constant(false)
                        )
                    ),
                    [
                        MacAlertButton(Text(verbatim: "Unpin")),
                        MacAlertButton(Text(verbatim: "Cancel"), role: .cancel),
                    ]
                )
            }
        },
        component(
            "mac.component.sheet.default",
            width: 600,
            height: 240
        ) {
            OverWindow.sheet(
                MacSheet("Set up sync", width: 460) {
                    Text(verbatim: "Content")
                        .foregroundStyle(.secondary)
                        .padding(ThemeSpace.group)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .overlay {
                            RoundedRectangle(cornerRadius: ThemeRadius.card)
                                .strokeBorder(
                                    Theme.hairlineStrong,
                                    style: StrokeStyle(
                                        lineWidth: 1,
                                        dash: [4, 3]
                                    )
                                )
                        }
                } footer: {
                    MacSheetFooter {
                        Button("Cancel") {}
                        Button("Set up") {}.buttonStyle(PrimaryButtonStyle())
                    }
                },
                over: Rectangle().fill(Theme.background)
            )
        },
        component("mac.component.modal.default", width: 1_000, height: 260) {
            MacModal(onDismiss: {}) {
                MacModalHeader(title: "Storage") {
                    MacButton("Done") {}
                }
                .frame(width: 360, height: 200, alignment: .top)
                .background(Theme.background)
            }
            .frame(height: 260)
        },
    ]

    /// An alert hung where `OverWindow` hangs one, under the top of a window
    /// background `height` tall.
    private static func alertOver(
        height: CGFloat,
        _ words: MacAlertWords,
        _ buttons: [MacAlertButton]
    ) -> some View {
        ZStack(alignment: .top) {
            Rectangle().fill(Theme.background)
            MacAlert(words, buttons: buttons)
                .elevatedFace()
                .padding(.top, OverWindow.hangs)
        }
        .frame(height: height)
    }

    /// `MacBackButton`, back to the page before.
    private static let backButtonScenes: [ShotScene] = [
        component("mac.component.back-button.default", page: true, height: 60) {
            MacBackButton(backTitle: "Playlists", onBack: {})
        }
    ]

    /// `MacQueueInsertLine`, where a dragged track will land.
    private static let queueInsertLineScenes: [ShotScene] = [
        component(
            "mac.component.queue-insert-line.default",
            page: true,
            height: 40
        ) {
            MacQueueInsertLine().frame(width: 380)
        }
    ]

    /// `Artwork`: a cover, the tile of a release with none, and a round
    /// picture.
    private static let artworkScenes: [ShotScene] = [
        component(
            "mac.component.artwork.cover",
            height: 136,
            images: coverStore()
        ) {
            Artwork(imageRef: cover("Low Tide Hours"), pointSize: 120)
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
        },
        component(
            "mac.component.artwork.missing",
            height: 136,
            images: coverStore()
        ) {
            Artwork(imageRef: nil, pointSize: 120)
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
        },
        component(
            "mac.component.artwork.round",
            height: 64,
            images: coverStore()
        ) {
            Artwork(imageRef: cover("Mara Quill"), pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(Circle())
        },
    ]

    /// `Avatar`: a person's initials, and their picture.
    private static let avatarScenes: [ShotScene] = [
        component("mac.component.avatar.initials", height: 48) {
            Avatar(initials: "MQ", size: 32)
        },
        component(
            "mac.component.avatar.picture",
            height: 48,
            images: ImageStore(fetchAvatarBytes: {
                PreviewData.previewArtData($0)
            })
        ) {
            Avatar(
                initials: "MQ",
                picture: (person: "Mara", version: "1"),
                size: 32
            )
        },
    ]

    /// `Logo`, the sheep over the word: at rest, and floating over its glow.
    private static let logoScenes: [ShotScene] = [
        component("mac.component.logo.pair", height: 180) {
            Logo(size: 120, bob: false, glow: false)
        }
    ]

    /// `PlaylistPicture`: four releases' covers, fewer than four, and none.
    private static let playlistPictureScenes: [ShotScene] = [
        component(
            "mac.component.playlist-picture.four-releases",
            height: 136,
            images: coverStore()
        ) {
            playlistPicture(
                .grid(
                    covers: [
                        cover("Low Tide Hours"), cover("Copper Sky"),
                        cover("Small Hours"), cover("Paper Lanterns"),
                    ]
                )
            )
        },
        component(
            "mac.component.playlist-picture.fewer-than-four",
            height: 136,
            images: coverStore()
        ) {
            playlistPicture(.single(cover: cover("Copper Sky")))
        },
        component(
            "mac.component.playlist-picture.empty",
            height: 136,
            images: coverStore()
        ) {
            playlistPicture(.single(cover: nil))
        },
    ]

    private static func playlistPicture(_ cover: BridgePlaylistCover)
        -> some View
    {
        PlaylistPicture(cover: cover, size: 120)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
    }

    /// `Stars`, rated four and not rated.
    private static let starsScenes: [ShotScene] = [
        component("mac.component.stars.rated", height: 40) {
            Stars(value: 4, size: 16)
        },
        component("mac.component.stars.not-rated", height: 40) {
            Stars(value: 0, size: 16)
        },
    ]

    /// `StatusChip` in each tone.
    private static let statusChipScenes: [ShotScene] = [
        statusChip("mac.component.status-chip.neutral", .neutral),
        statusChip("mac.component.status-chip.accent", .accent),
        statusChip("mac.component.status-chip.info", .info),
        statusChip("mac.component.status-chip.success", .success),
        statusChip("mac.component.status-chip.warning", .warning),
        statusChip("mac.component.status-chip.danger", .danger),
        statusChip("mac.component.status-chip.activity", .activity),
    ]

    private static func statusChip(_ id: String, _ tone: StatusTone)
        -> ShotScene
    {
        component(id, height: 40) {
            StatusChip(verbatim: "Synced", tone: tone)
        }
    }
}
