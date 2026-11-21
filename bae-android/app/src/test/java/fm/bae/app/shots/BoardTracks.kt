package fm.bae.app.shots

/** A track as the boards list it: its title, the album it is on, which names its artist, and its length. */
internal class BoardTrack(
    val title: String,
    val album: BoardAlbum,
    private val minutes: Int,
    private val seconds: Int,
) {
    val artist: String get() = album.artist
    val durationMs: ULong get() = ((minutes * SECONDS_PER_MINUTE + seconds) * MILLIS_PER_SECOND).toULong()
}

private const val SECONDS_PER_MINUTE = 60
private const val MILLIS_PER_SECOND = 1_000

private val lowTideHours = boardAlbumFacts[0]
private val glassHarbor = boardAlbumFacts[2]
private val paperLanterns = boardAlbumFacts[1]
private val copperSky = boardAlbumFacts[3]
private val smallHours = boardAlbumFacts[7]

/** The tracks of "Late night", in the order the playlist boards list them. */
internal val lateNightTracks =
    listOf(
        BoardTrack("Harbor Lights", lowTideHours, 4, 32),
        BoardTrack("Copper Sky", copperSky, 3, 58),
        BoardTrack("Night Ferry", lowTideHours, 4, 36),
        BoardTrack("Small Hours", smallHours, 6, 10),
        BoardTrack("Paper Lanterns", paperLanterns, 5, 1),
        BoardTrack("Slow Morning", lowTideHours, 10, 22),
        BoardTrack("Harbor Wall", glassHarbor, 3, 12),
        BoardTrack("Lantern Walk", paperLanterns, 2, 58),
        BoardTrack("Copper Wire", copperSky, 3, 40),
        BoardTrack("Last Ferry Home", smallHours, 3, 5),
        BoardTrack("Salt Glass", glassHarbor, 3, 31),
        BoardTrack("Morning Tide", lowTideHours, 3, 15),
    )
