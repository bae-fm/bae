package fm.bae.app.shots

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.unit.dp
import fm.bae.app.data.ImageContent
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.components.Avatar
import fm.bae.app.ui.components.AvatarFace
import org.junit.Test

/**
 * The component shots of `Avatar`, one for each row of its preview. The initials are the preview's Android row, which
 * keeps CSS's centring as Compose draws it; its Initials row is the Mac's.
 */
class AvatarShots : BoardShot() {
    @Test
    fun androidComponentAvatarAndroidInitials() =
        component("android.component.avatar.android-initials") { Avatar(AvatarFace(initials = "MQ", picture = null), size = 32.dp) }

    @Test
    fun androidComponentAvatarPicture() =
        component("android.component.avatar.picture") {
            CompositionLocalProvider(LocalImageStore provides avatarStore()) {
                Avatar(AvatarFace(initials = "MQ", picture = ImageContent.Avatar(person = "mara", version = "1")), size = 32.dp)
            }
        }
}
