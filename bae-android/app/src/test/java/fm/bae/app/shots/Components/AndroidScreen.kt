package fm.bae.app.shots

import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.library.WhoseMenu
import org.junit.Test
import uniffi.bae_bridge.BridgeHistoryShown

/** The component shots of `AndroidScreen`, one for each row of its preview. */
class AndroidScreenShots : BoardShot() {
    @Test
    fun androidComponentScreenBack() =
        component("android.component.screen.back") {
            AndroidScreen(title = "Storage", onBack = {}, modifier = Modifier.height(360.dp)) {
                Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

    @Test
    fun androidComponentScreenNoBack() =
        component("android.component.screen.no-back") {
            AndroidScreen(title = "Storage", onBack = null, modifier = Modifier.height(360.dp)) {
                Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

    @Test
    fun androidComponentScreenMenu() =
        component("android.component.screen.menu") {
            AndroidScreen(
                title = "History",
                onBack = {},
                modifier = Modifier.height(360.dp),
                actions = { WhoseMenu(menu = historyChoice(BridgeHistoryShown.Everyone), open = false, onOpen = {}, onChoose = {}) },
            ) {
                Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
            }
        }

    @Test
    fun androidComponentScreenDialog() =
        component("android.component.screen.dialog") {
            InPlaceDialogScene(
                behind = {
                    AndroidScreen(title = "Storage", onBack = {}) {
                        Text("The screen’s content", modifier = Modifier.padding(ThemeSpace.edge))
                    }
                },
            ) {
                DialogParts(
                    title = "Remove library from this device?",
                    action = DialogAction("Remove", {}),
                    dismiss = DialogAction("Cancel", {}),
                )
            }
        }

    @Test
    fun androidComponentScreenKeyboard() =
        shot("android.component.screen.keyboard") {
            KeyboardScene {
                AndroidScreen(title = "Name", onBack = {}) {
                    val focus = remember { FocusRequester() }
                    AndroidTextField(
                        value = "Home",
                        onValueChange = {},
                        label = "Library name",
                        modifier = Modifier.padding(ThemeSpace.edge).focusRequester(focus),
                    )
                    LaunchedEffect(Unit) { focus.requestFocus() }
                }
            }
        }
}
