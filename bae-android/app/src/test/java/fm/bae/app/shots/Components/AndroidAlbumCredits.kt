package fm.bae.app.shots

import fm.bae.app.creditLineNamed
import fm.bae.app.ui.albumdetail.AndroidAlbumCredits
import org.junit.Test
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeCreditRow
import uniffi.bae_bridge.BridgeTermLabel

/** The component shots of `AndroidAlbumCredits`, one for each row of its preview. */
class AndroidAlbumCreditsShots : BoardShot() {
    @Test
    fun androidComponentAlbumCreditsCredits() =
        component("android.component.album-credits.credits", wide = true) {
            AndroidAlbumCredits(
                listOf(credit("Producer", "Ines Varga"), credit("Engineer", "Pell Marsh"), credit("Arranger", "Ada Lindqvist")),
            )
        }

    @Test
    fun androidComponentAlbumCreditsLongNames() =
        component("android.component.album-credits.long-names", wide = true) {
            AndroidAlbumCredits(
                listOf(
                    credit("Producer", "Ines Varga and Tomas Brell"),
                    credit("Mastering engineer", "Pell Marsh at Harbor Room Studios"),
                ),
            )
        }
}

private fun credit(
    role: String,
    name: String,
) = BridgeCreditRow(
    role = BridgeCreditRole(BridgeTermLabel.Verbatim(role), emptyList()),
    artists = creditLineNamed(name),
    tracks = emptyList(),
)
