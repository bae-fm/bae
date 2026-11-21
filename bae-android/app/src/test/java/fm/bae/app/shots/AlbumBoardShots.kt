package fm.bae.app.shots

import androidx.compose.foundation.layout.RowScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.creditLineNamed
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AlbumDetailCallbacks
import fm.bae.app.ui.albumdetail.AlbumDetailContent
import fm.bae.app.ui.albumdetail.AlbumMenuActions
import fm.bae.app.ui.albumdetail.AlbumOptionsMenu
import fm.bae.app.ui.albumdetail.AlbumPlaybackState
import fm.bae.app.ui.albumdetail.unavailableReleaseHeading
import fm.bae.app.ui.albumdetail.AlbumRatingChoices
import fm.bae.app.ui.library.AndroidDetailScreen
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeAudioFormat
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeCreditRow
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeDurationUnits
import uniffi.bae_bridge.BridgeFile
import uniffi.bae_bridge.BridgeFileKind
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeOtherDevice
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeReleaseRecord
import uniffi.bae_bridge.BridgeSourceAudioDescriptor
import uniffi.bae_bridge.BridgeSourceAudioLayout
import uniffi.bae_bridge.BridgeSourceAudioSummary
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeTrackSpan
import uniffi.bae_bridge.BridgeTrackWork

/** The shots of the album page's boards: the page with its rating, its works and credits, its catalog links and files. */
class AlbumBoardShots : BoardShot() {
    @Test
    fun androidAlbumRating() =
        shot("android.album.rating") {
            AlbumPage(
                boardAlbum(
                    rating =
                        BridgeAlbumRating(
                            mine = BridgeStars.FOUR,
                            raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
                        ),
                ),
            )
        }

    @Test
    fun androidAlbumPage() = shot("android.album.page") { AlbumPage(boardAlbum()) }

    @Test
    fun androidAlbumRate() =
        shot("android.album.rate") {
            val rating =
                BridgeAlbumRating(
                    mine = BridgeStars.FOUR,
                    raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
                )
            SheetScene(title = stringResource(R.string.rate), behind = { AlbumPage(boardAlbum(rating = rating)) }) {
                AlbumRatingChoices(rating = rating, onRate = {})
            }
        }

    @Test
    fun androidAlbumCredits() =
        shot("android.album.credits") {
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
            AlbumPage(detail.copy(releases = listOf(credited)))
        }

    @Test
    fun androidAlbumLinksAndFiles() =
        shot("android.album.links-and-files") {
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
            AlbumPage(detail.copy(releases = listOf(release))) {
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

/** "Low Tide Hours" by Mara Quill, a CD on Harbor Records with the six tracks the boards list, one disc, with [rating] as its rating. */
internal fun boardAlbum(rating: BridgeAlbumRating = BridgeAlbumRating(mine = null, raters = emptyList())): BridgeAlbumDetail {
    val titles = listOf("Harbor Lights", "The Long Way Home", "Second Floor", "Low Tide Hours", "Tin Roof", "Margins")
    val tracks = titles.mapIndexed { index, title -> PreviewData.track("trk-${index + 1}", title, index + 1) }
    val album = PreviewData.album(title = "Low Tide Hours", year = 1992, artistNames = "Mara Quill")
    val release =
        harborRelease.copy(
            id = "rel-alb-1",
            albumId = album.id,
            tracks = tracks,
            trackGroups = listOf(PreviewData.trackGroup(tracks)),
        )
    return PreviewData.albumDetail(album, listOf(release)).copy(rating = rating)
}

private val unavailableDevice = BridgeOtherDevice.Active("Ana’s Mac")
private val unavailablePlayability = BridgeReleasePlayability.OnOtherDevice(unavailableDevice)

/** The album the boards draw as held by Ana's Mac only: ten tracks, none of which this device can play. */
private val onAnotherDevice: BridgeAlbumDetail =
    harborDetail.let { detail ->
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
            ).mapIndexed { index, (title, seconds) ->
                PreviewData.track("trk-${index + 1}", title, index + 1).copy(
                    durationMs = seconds.toULong() * 1_000UL,
                    durationClock = BridgeDurationClock(false, null, (seconds / 60).toUInt(), (seconds % 60).toUInt()),
                )
            }
        val release =
            detail.releases[0].copy(
                trackGroups = listOf(PreviewData.trackGroup(tracks)),
                tracks = tracks,
                playability = unavailablePlayability,
                totalDuration = BridgeDurationUnits.MinutesOnly(52UL),
            )
        detail.copy(
            releases = listOf(release),
            shownRelease = release,
            rating = BridgeAlbumRating(mine = null, raters = emptyList()),
        )
    }

/** The track playing in the bar behind the album page, 1:28 into 4:51. */
private val northbound =
    PlayerStaging(elapsedSeconds = 88, held = listOf(BridgeTrackSpan(0.0, 0.3)), title = "Northbound", durationSeconds = 291)

@Composable
private fun UnavailableAlbumPage() {
    AlbumPageContent(onAnotherDevice, northbound, catalogName = { it.name }) {
        AlbumOptionsMenu(
            AlbumMenuActions(play = null, onRate = {}, pin = null, onSetPrimaryRelease = null, onDelete = {}),
        )
    }
}

/** The album page as the boards draw it: its detail screen, with [bar]'s track playing in its bar. */
@Composable
internal fun AlbumPage(
    detail: BridgeAlbumDetail,
    bar: PlayerStaging = playing,
    catalogName: (BridgeCatalog) -> String = { it.name },
) {
    ScreenScene { AlbumPageContent(detail, bar, catalogName) }
}

@Composable
private fun AlbumPageContent(
    detail: BridgeAlbumDetail,
    bar: PlayerStaging,
    catalogName: (BridgeCatalog) -> String,
    actions: @Composable RowScope.() -> Unit = {},
) {
    CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
        AndroidDetailScreen(
            onBack = {},
            backLabel = stringResource(R.string.back_to_library),
            nowPlayingBar = { Bar(bar) },
            actions = actions,
        ) {
            AlbumDetailContent(
                detail = detail,
                cover = null,
                playback = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
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
                catalogName = catalogName,
                audioChannelsKey = { "core.audio.channels.stereo" },
            )
        }
    }
}
