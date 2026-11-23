package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.creditLineNamed
import fm.bae.app.heartersText
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AlbumDetailCallbacks
import fm.bae.app.ui.albumdetail.AlbumDetailContent
import fm.bae.app.ui.albumdetail.AlbumMenuActions
import fm.bae.app.ui.albumdetail.AlbumMenuItems
import fm.bae.app.ui.albumdetail.AlbumOptionsMenu
import fm.bae.app.ui.albumdetail.AlbumPlaybackState
import fm.bae.app.ui.albumdetail.AlbumRatingAction
import fm.bae.app.ui.albumdetail.AlbumRatingChoices
import fm.bae.app.ui.albumdetail.PinMenuItem
import fm.bae.app.ui.albumdetail.PlayMenuActions
import fm.bae.app.ui.albumdetail.albumRatingHeading
import fm.bae.app.ui.albumdetail.unavailableReleaseHeading
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidMenuItemStyle
import fm.bae.app.ui.library.AndroidDetailScreen
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeAudioFormat
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeCreditRow
import uniffi.bae_bridge.BridgeFile
import uniffi.bae_bridge.BridgeFileKind
import uniffi.bae_bridge.BridgeHearter
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeNowPlayingDisplay
import uniffi.bae_bridge.BridgeOtherDevice
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeReleaseRecord
import uniffi.bae_bridge.BridgeSourceAudioDescriptor
import uniffi.bae_bridge.BridgeSourceAudioLayout
import uniffi.bae_bridge.BridgeSourceAudioSummary
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeTrackDisplay
import uniffi.bae_bridge.BridgeTrackSpan
import uniffi.bae_bridge.BridgeTrackWork

/** The shots of the album page's boards: the page with its rating, its works and credits, its catalog links and files. */
class AlbumBoardShots : BoardShot() {
    @Test
    fun androidAlbumMenu() =
        shot("android.album.menu") {
            AlbumPageWithMenu(boardAlbum(), rating = AlbumRatingAction.Rate {})
        }

    @Test
    fun androidAlbumRating() =
        shot("android.album.rating") {
            AlbumPage(
                boardAlbum(
                    rating =
                        BridgeAlbumRating.Stars(
                            mine = BridgeStars.FOUR,
                            raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
                        ),
                ),
            )
        }

    @Test
    fun androidAlbumRatingHeart() =
        shot("android.album.rating.heart") {
            val hearters = listOf(BridgeHearter.You, BridgeHearter.Person("Ana"))
            val rating = BridgeAlbumRating.Heart(hearted = true, hearters = hearters)
            val heart = AlbumRatingAction.Heart(LocalContext.current.heartersText(hearters), hearted = true, onChoose = {})
            AlbumPageWithMenu(boardAlbum(rating, trackCount = 5), heart)
        }

    @Test
    fun androidAlbumRatingNone() =
        shot("android.album.rating.none") {
            AlbumPageWithMenu(boardAlbum(BridgeAlbumRating.Hidden, trackCount = 5), rating = null)
        }

    @Test
    fun androidAlbumPage() =
        shot("android.album.page") {
            AlbumPage(boardAlbum(), playback = AlbumPlaybackState(playingLowTideHours.track.trackId, isPlaying = true))
        }

    @Test
    fun androidAlbumRate() =
        shot("android.album.rate") {
            val rating =
                BridgeAlbumRating.Stars(
                    mine = BridgeStars.FOUR,
                    raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
                )
            SheetScene(
                heading = albumRatingHeading(rating),
                behind = { AlbumPage(boardAlbum(rating = rating)) },
            ) {
                AlbumRatingChoices(rating = rating, onRate = {})
            }
        }

    @Test
    fun androidAlbumCredits() =
        shot("android.album.credits", TALL_BOARD) {
            val detail = boardAlbum()
            val release = detail.releases[0]
            val work = BridgeTrackWork("work-ebb", "Low Tide Suite: I. Ebb", "Ada Lindqvist")
            val groups =
                release.trackGroups.map { group ->
                    group.copy(tracks = group.tracks.map { if (it.id == "trk-4") it.copy(works = listOf(work)) else it })
                }
            val credits =
                listOf("Producer" to "Ines Varga", "Engineer" to "Pell Marsh", "Arranger" to "Ada Lindqvist").map { (role, name) ->
                    BridgeCreditRow(
                        role = BridgeCreditRole(BridgeTermLabel.Verbatim(role), emptyList()),
                        artists = creditLineNamed(name),
                        tracks = emptyList(),
                    )
                }
            val flac =
                BridgeAudioFormat(codec = "FLAC", sampleRateHz = 44_100, bitsPerSample = 16, bitrateKbps = null, channels = 2)
            val credited =
                release.copy(
                    trackGroups = groups,
                    tracks = groups.flatMap { it.tracks },
                    credits = credits,
                    sourceAudio = BridgeSourceAudioSummary.Uniform(BridgeSourceAudioDescriptor(BridgeSourceAudioLayout.FILE, flac)),
                )
            AlbumPage(detail.copy(releases = listOf(credited), shownRelease = credited))
        }

    @Test
    fun androidAlbumLinksAndFiles() =
        shot("android.album.links-and-files", TALL_BOARD) {
            val detail = boardAlbum()
            val records =
                listOf(BridgeCatalog.MUSIC_BRAINZ, BridgeCatalog.DISCOGS, BridgeCatalog.WIKIDATA).map {
                    BridgeReleaseRecord(catalog = it, url = "https://records.example/${it.name}")
                }
            val files =
                listOf(
                    "Low Tide Hours.cue" to BridgeFileKind.CueSheet,
                    "Low Tide Hours.log" to BridgeFileKind.RipLog,
                    "booklet.pdf" to BridgeFileKind.Pdf,
                ).map { (name, kind) -> boardFile(name, kind) }
            val release = detail.releases[0].copy(records = records, hasRecords = true, otherFiles = files, hasOtherFiles = true)
            AlbumPage(detail.copy(releases = listOf(release), shownRelease = release)) {
                when (it) {
                    BridgeCatalog.MUSIC_BRAINZ -> "MusicBrainz"
                    BridgeCatalog.DISCOGS -> "Discogs"
                    else -> "Wikidata"
                }
            }
        }

    @Test
    fun androidAlbumPageOnAnotherDevice() =
        shot("android.album.page.on-another-device") {
            ScreenScene { UnavailableAlbumPage() }
        }

    @Test
    fun androidAlbumOnAnotherDeviceSheet() =
        shot("android.album.on-another-device-sheet") {
            val message = BridgeMessage("core.release.on_device", listOf(BridgeMessageArg("device", unavailableDevice.name)))
            SheetScene(heading = unavailableReleaseHeading(message), behind = { UnavailableAlbumPage() }) {}
        }
}

/** The height of the boards whose screens hold the ten tracks and what follows them, in dp. */
private val TALL_BOARD = 1160.dp

private val ALBUM_MENU_TOP = 48.dp

/** A file of the release's folder, without audio or image content. */
internal fun boardFile(
    name: String,
    kind: BridgeFileKind,
) = BridgeFile(
    id = name,
    originalFilename = name,
    name = name,
    folder = null,
    kind = kind,
    fileSize = 1_000UL,
    contentType = "application/octet-stream",
    isImage = false,
    audioFormat = null,
)

/**
 * "Low Tide Hours" by Mara Quill, a CD on Harbor Records with the first [trackCount] tracks the boards list, one disc,
 * with [rating] as its rating.
 */
internal fun boardAlbum(
    rating: BridgeAlbumRating = BridgeAlbumRating.Stars(mine = null, raters = emptyList()),
    trackCount: Int = 10,
): BridgeAlbumDetail {
    val tracks =
        listOf(
            "Harbor Lights" to 272,
            "The Long Way Home" to 312,
            "Second Floor" to 225,
            "Low Tide Hours" to 303,
            "Tin Roof" to 341,
            "Margins" to 295,
            "Weather Report Card" to 276,
            "Old Dog" to 177,
            "Night Ferry" to 276,
            "Slow Morning" to 622,
        ).take(trackCount).mapIndexed { index, (title, seconds) ->
            val durationMs = seconds.toULong() * 1_000uL
            PreviewData.track("trk-${index + 1}", title, index + 1).copy(
                durationMs = durationMs,
                durationClock = PreviewData.durationClock(durationMs),
            )
        }
    val album = PreviewData.album(title = "Low Tide Hours", year = 1992, artistNames = "Mara Quill")
    val release =
        harborRelease.copy(
            id = "rel-alb-1",
            albumId = album.id,
            cover = PreviewData.imageRef(album.title),
            tracks = tracks,
            trackGroups = listOf(PreviewData.trackGroup(tracks)),
        )
    return PreviewData.albumDetail(album, listOf(release)).copy(rating = rating)
}

private val unavailableDevice = BridgeOtherDevice.Active("Ana’s Mac")

/** The album the boards draw as held by Ana's Mac only: ten tracks, none of which this device can play. */
private val onAnotherDevice: BridgeAlbumDetail =
    boardAlbum().let { detail ->
        val release = detail.shownRelease.copy(playability = BridgeReleasePlayability.OnOtherDevice(unavailableDevice))
        detail.copy(releases = listOf(release), shownRelease = release)
    }

/** The track playing in the bar behind the album page, 1:28 into 4:51. */
internal val playingNorthbound =
    PlayerStaging(
        elapsedSeconds = 88,
        held = listOf(BridgeTrackSpan(0.0, 0.3)),
        track = northboundTrack(title = "Northbound", cover = PreviewData.imageRef("Northbound")),
        durationSeconds = 291,
    )

@Composable
private fun UnavailableAlbumPage() {
    AlbumPageContent(onAnotherDevice, playingNorthbound, catalogName = { it.name }) {
        AlbumOptionsMenu(
            AlbumMenuActions(
                play = null,
                rating = { AlbumRatingAction.Rate {} },
                pin = null,
                onSetPrimaryRelease = null,
                onDelete = {},
            ),
        )
    }
}

/** The title track of the release shown on both the album and playlist boards. */
internal val playingLowTideHours =
    boardAlbum().let { detail ->
        val release = detail.shownRelease
        val track = release.tracks.single { it.title == detail.album.title }
        PlayerStaging(
            elapsedSeconds = 102,
            held = listOf(BridgeTrackSpan(0.0, 0.6)),
            track =
                NowPlaying(
                    trackId = track.id,
                    display =
                        BridgeNowPlayingDisplay.Library(
                            BridgeTrackDisplay(
                                title = track.title,
                                artistLine = detail.album.artistLine,
                                albumId = detail.album.id,
                                releaseId = release.id,
                                albumTitle = detail.album.title,
                                coverImage = release.cover,
                            ),
                        ),
                    sidePausePrompt = null,
                ),
        )
    }

private val boardMenu =
    AlbumMenuActions(
        play = PlayMenuActions(onShuffle = {}, onPlayNext = {}, onAddToQueue = {}),
        rating = { AlbumRatingAction.Rate {} },
        pin = null,
        onSetPrimaryRelease = null,
        onDelete = {},
    )

@Composable
private fun AlbumPageWithMenu(
    detail: BridgeAlbumDetail,
    rating: AlbumRatingAction?,
) {
    val menu =
        AlbumMenuActions(
            play = boardMenu.play,
            rating = { rating },
            pin = PinMenuItem(pinned = true, onClick = {}),
            onSetPrimaryRelease = null,
            onDelete = {},
        )
    ScreenScene {
        Box {
            val playback = AlbumPlaybackState(playingLowTideHours.track.trackId, isPlaying = true)
            AlbumPageContent(detail, playingLowTideHours, { it.name }, playback) { AlbumOptionsMenu(menu) }
            OpenMenu(top = ALBUM_MENU_TOP, end = ThemeSpace.related) { AndroidMenuItemStyle { AlbumMenuItems(menu, close = {}) } }
        }
    }
}

/** The album page as the boards draw it: its detail screen, with [bar]'s track playing in its bar. */
@Composable
internal fun AlbumPage(
    detail: BridgeAlbumDetail,
    playback: AlbumPlaybackState = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
    catalogName: (BridgeCatalog) -> String = { it.name },
) {
    ScreenScene { AlbumPageContent(detail, playingLowTideHours, catalogName, playback) { AlbumOptionsMenu(boardMenu) } }
}

@Composable
private fun AlbumPageContent(
    detail: BridgeAlbumDetail,
    bar: PlayerStaging,
    catalogName: (BridgeCatalog) -> String,
    playback: AlbumPlaybackState = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
    actions: @Composable RowScope.() -> Unit,
) {
    Covers {
        AndroidDetailScreen(
            onBack = {},
            backLabel = stringResource(R.string.back_to_library),
            nowPlayingBar = { Bar(bar) },
            actions = actions,
        ) {
            Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                AlbumDetailContent(
                    detail = detail,
                    cover = detail.shownRelease.cover,
                    playback = playback,
                    callbacks =
                        AlbumDetailCallbacks(
                            onSelectRelease = {},
                            onTogglePlayPause = {},
                            onPlayTrack = {},
                            onPlayRelease = {},
                            onShuffleRelease = {},
                            onPlayReleaseNext = {},
                            onAddReleaseToQueue = {},
                            onPlayTrackNext = {},
                            onAddTrackToQueue = {},
                            onRateAlbum = {},
                            onSelectWork = {},
                        ),
                    releaseDownloadControl = {},
                    shownRating = { detail.rating },
                    catalogName = catalogName,
                    audioChannelsKey = { "core.audio.channels.stereo" },
                )
            }
        }
    }
}
