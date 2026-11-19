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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import fm.bae.app.data.ImageContent
import fm.bae.app.ui.appearance.ThemeOpacity
import uniffi.bae_bridge.BridgePersonProfile

/** What an [AndroidAvatar] draws: the person's [name], whose initials stand in while they have no [picture]. */
internal class AvatarFace(
    val name: String,
    val picture: ImageContent.Avatar?,
)

/** The face of [this] person: their picture at the version their profile names, when they have one. */
internal fun BridgePersonProfile.face(): AvatarFace =
    AvatarFace(name, avatarVersion?.let { ImageContent.Avatar(person = id, version = it) })

/** A person's picture, or their initials in the accent on its tint while they have none. Round. */
@Composable
internal fun AndroidAvatar(
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
                .background(accent.copy(alpha = ThemeOpacity.tintStrong)),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = initialsOf(face.name),
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

/** The first letter of up to two words of [name], upper case. */
internal fun initialsOf(name: String): String =
    name
        .split(Regex("\\s+"))
        .filter { it.isNotEmpty() }
        .take(2)
        .joinToString("") { it.first().uppercase() }
