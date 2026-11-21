package fm.bae.app.shots

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.unit.dp
import fm.bae.app.data.ImageContent
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.components.Avatar
import fm.bae.app.ui.components.AvatarFace
import org.junit.Test

/** The component shots of `Avatar`, one for each row of its preview. */
class AvatarShots : BoardShot() {
    @Test
    fun androidComponentAvatarInitials() =
        component("android.component.avatar.initials") { Avatar(AvatarFace(initials = "MQ", picture = null), size = 32.dp) }

    @Test
    fun androidComponentAvatarPicture() =
        component("android.component.avatar.picture") {
            CompositionLocalProvider(LocalImageStore provides avatarStore()) {
                Avatar(AvatarFace(initials = "MQ", picture = ImageContent.Avatar(person = "mara", version = "1")), size = 32.dp)
            }
        }
}
