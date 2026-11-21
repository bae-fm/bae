package fm.bae.app.shots

import androidx.compose.foundation.layout.width
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.AndroidErrorDetails
import org.junit.Test

/** The component shots of `AndroidErrorDetails`, one for each row of its preview. */
class AndroidErrorDetailsShots : BoardShot() {
    @Test
    fun androidComponentErrorDetailsCauses() =
        component("android.component.error-details.causes") {
            AndroidErrorDetails(
                text = "The sign-in settings file is missing.\nOAuthConfig: missing client id",
                copyText = "The sign-in settings file is missing.\nOAuthConfig: missing client id",
                modifier = Modifier.width(320.dp),
            )
        }
}
