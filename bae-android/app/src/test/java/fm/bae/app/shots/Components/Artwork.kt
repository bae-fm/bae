package fm.bae.app.shots

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.Artwork
import org.junit.Test

/** The component shots of `Artwork`, one for each row of its preview. */
class ArtworkShots : BoardShot() {
    @Test
    fun androidComponentArtworkCover() =
        component("android.component.artwork.cover") {
            Covers {
                Artwork(
                    cover = PreviewData.imageRef("Low Tide Hours"),
                    cornerRadius = ThemeRadius.artwork,
                    iconPadding = ThemeSpace.group,
                    modifier = Modifier.size(120.dp),
                )
            }
        }

    @Test
    fun androidComponentArtworkMissing() =
        component("android.component.artwork.missing") {
            Images {
                Artwork(cover = null, cornerRadius = ThemeRadius.artwork, iconPadding = ThemeSpace.group, modifier = Modifier.size(120.dp))
            }
        }

    /** An artist's picture, round, as the artist heading draws it. */
    @Test
    fun androidComponentArtworkRound() =
        component("android.component.artwork.round") {
            Covers {
                Artwork(
                    cover = PreviewData.imageRef("Mara Quill"),
                    cornerRadius = 24.dp,
                    iconPadding = ThemeSpace.related,
                    modifier = Modifier.size(48.dp),
                )
            }
        }

    @Test
    fun androidComponentArtworkRing() =
        component("android.component.artwork.ring") {
            Covers {
                Artwork(
                    cover = PreviewData.imageRef("Low Tide Hours"),
                    cornerRadius = ThemeRadius.artwork,
                    iconPadding = ThemeSpace.group,
                    modifier = Modifier.size(120.dp).border(2.dp, MaterialTheme.colorScheme.primary),
                )
            }
        }

    @Test
    fun androidComponentArtworkShadowTile() =
        component("android.component.artwork.shadow-tile") {
            Covers {
                Artwork(
                    cover = PreviewData.imageRef("Low Tide Hours"),
                    cornerRadius = ThemeRadius.artwork,
                    iconPadding = ThemeSpace.group,
                    modifier = Modifier.size(120.dp).shadow(6.dp),
                )
            }
        }

    @Test
    fun androidComponentArtworkShadowBar() =
        component("android.component.artwork.shadow-bar") {
            Covers {
                Artwork(
                    cover = PreviewData.imageRef("Low Tide Hours"),
                    cornerRadius = ThemeRadius.artwork,
                    iconPadding = ThemeSpace.group,
                    modifier = Modifier.size(120.dp).shadow(3.dp),
                )
            }
        }
}
