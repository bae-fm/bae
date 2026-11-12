package fm.bae.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.DisplayError
import fm.bae.app.coreString

/** "Something went wrong." where a broken guarantee left nothing to show: the bug is already reported. */
@Composable
internal fun SomethingWentWrongNotice() {
    AndroidNotice(DisplayError(LocalContext.current.coreString("core.error.category.internal")))
}
