package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.ui.albumdetail.AlbumDetailScene
import fm.bae.app.ui.library.LibraryGridScene
import fm.bae.app.ui.onboarding.WelcomeScene

// Screenshot scenes captured by scripts/shots/android.sh. Each function is one
// scene: the plugin renders it to a PNG named after the function, and the script
// copies it to <scene-id>@android.png. Every scene renders a shared, session-free
// composition from the production UI (defined in main next to its @Preview) in
// the app chrome — never a capture-only re-layout. Rendered at a current phone
// size in dark mode to match the shipped app. Function name ⇒ scene id:
//   Welcome ⇒ welcome, LibraryGrid ⇒ library-grid, AlbumDetail ⇒ album-detail.

private const val PHONE_SPEC = "spec:width=411dp,height=914dp,dpi=420"

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun Welcome() {
    WelcomeScene()
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGrid() {
    LibraryGridScene()
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun AlbumDetail() {
    AlbumDetailScene()
}
