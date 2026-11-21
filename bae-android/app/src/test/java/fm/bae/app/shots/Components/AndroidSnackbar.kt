package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.components.AndroidSnackbarHost
import fm.bae.app.ui.components.SnackbarLift
import org.junit.Test

/** The component shots of `AndroidSnackbar`, one for each row of its preview. */
class AndroidSnackbarShots : BoardShot() {
    @Test
    fun androidComponentSnackbarDefault() =
        component("android.component.snackbar.default", wide = true) {
            val state = remember { SnackbarHostState() }
            LaunchedEffect(Unit) {
                state.showSnackbar("Could not reach the cloud.", actionLabel = "Retry", duration = SnackbarDuration.Indefinite)
            }
            Box(Modifier.fillMaxWidth().height(SNACKBAR_HEIGHT)) { AndroidSnackbarHost(state = state, lift = remember { SnackbarLift() }) }
        }
}

private val SNACKBAR_HEIGHT = 72.dp
