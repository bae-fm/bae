package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.albumdetail.AlbumDetailScene
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore
import fm.bae.app.ui.appearance.SurfaceTone
import fm.bae.app.ui.library.LibraryGridScene
import fm.bae.app.ui.onboarding.WelcomeScene
import fm.bae.app.ui.settings.AppearanceSection

// Screenshot scenes captured by scripts/shots/android.sh. Each function is one
// scene: the plugin renders it to a PNG named after the function, and the script
// copies it to <scene-id>@android.png. Every scene renders a shared, session-free
// composition from the production UI (defined in main next to its @Preview) in
// the app chrome — never a capture-only re-layout. Rendered at a current phone
// size, with additional light and tinted appearance captures. Function name ⇒ scene id:
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

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_NO)
@Composable
fun WelcomeLight() {
    WelcomeScene()
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_NO)
@Composable
fun LibraryGridLight() {
    LibraryGridScene()
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_NO)
@Composable
fun AlbumDetailLight() {
    AlbumDetailScene()
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightNeutral() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.NEUTRAL, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightSlate() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.SLATE, AccentChoice.TEAL)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightPlum() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.PLUM, AccentChoice.PURPLE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkNeutral() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.NEUTRAL, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkSlate() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.SLATE, AccentChoice.TEAL)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkPlum() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.PLUM, AccentChoice.PURPLE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightMidnight() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.MIDNIGHT, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkMidnight() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.MIDNIGHT, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightForest() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.FOREST, AccentChoice.GREEN)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkForest() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.FOREST, AccentChoice.GREEN)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightSand() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.SAND, AccentChoice.AMBER)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkSand() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.SAND, AccentChoice.AMBER)
}

@Composable
private fun AppearanceScene(
    mode: AppearanceMode,
    tone: SurfaceTone,
    accent: AccentChoice,
) {
    val store = remember(mode, tone, accent) { AppearanceStore(AppearancePreferences(mode, accent, tone)) {} }
    BaeTheme(appearance = store) {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            AppearanceSection()
        }
    }
}
