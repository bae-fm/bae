package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.settings.S3KeyReplacement
import fm.bae.app.ui.settings.StorageSection
import org.junit.Test
import uniffi.bae_bridge.BridgeMemberAccess

/** The component shots of `AndroidCloudHome`, one for each row of its preview. */
class AndroidCloudHomeShots : BoardShot() {
    @Test
    fun androidComponentCloudHomeResting() =
        component("android.component.cloud-home.resting", wide = true) { CloudHome(key = "AKIA4EXAMPLEOLD") }

    @Test
    fun androidComponentCloudHomeKeyReplaced() =
        component("android.component.cloud-home.key-replaced", wide = true) {
            CloudHome(key = "AKIA4EXAMPLENEW", keyLine = S3KeyReplacement.Line.Saved("AKIA4EXAMPLEOLD"))
        }
}

/** The S3 library's storage as Settings shows it, recording [key] for the person, with [keyLine] under it. */
@Composable
private fun CloudHome(
    key: String,
    keyLine: S3KeyReplacement.Line? = null,
) = StorageSection(
    location = sceneS3,
    access = BridgeMemberAccess.S3AccessKey(key),
    keyLine = { keyLine },
    onReplaceKey = {},
    onDismissKeyLine = {},
)
