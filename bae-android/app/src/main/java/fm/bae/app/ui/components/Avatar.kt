package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import fm.bae.app.data.ImageContent
import fm.bae.app.ui.appearance.ThemeOpacity
import uniffi.bae_bridge.BridgePersonProfile

/** What an [Avatar] draws: the [initials] that stand in while the person has no [picture]. */
internal class AvatarFace(
    val initials: String,
    val picture: ImageContent.Avatar?,
)

/** The face of [this] person: their picture at the version their profile names, when they have one. */
internal fun BridgePersonProfile.face(): AvatarFace =
    AvatarFace(initials, avatarVersion?.let { ImageContent.Avatar(person = id, version = it) })

/** A person's picture, or their initials in the accent on its tint while they have none. Round. */
@Composable
internal fun Avatar(
    face: AvatarFace,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    Box(
        modifier =
            modifier
                .size(size)
                .clip(CircleShape)
                .background(accent.copy(alpha = ThemeOpacity.tintStrong))
                // The row beside it names the person; the initials would read into that name.
                .clearAndSetSemantics {},
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = face.initials,
            color = accent,
            fontSize = (size.value * INITIALS_SHARE).sp,
            fontWeight = FontWeight.SemiBold,
        )
        face.picture?.let {
            ImageSlot(content = it, cornerRadius = size / 2, iconPadding = size / 2, modifier = Modifier.fillMaxSize())
        }
    }
}

/** The initials' size as a share of the avatar's. */
private const val INITIALS_SHARE = 0.4f
