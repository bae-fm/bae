package fm.bae.app.ui.components

import androidx.compose.foundation.layout.RowScope
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import fm.bae.app.ui.LocalPrimaryFill

/** Native button behavior with the shared primary fill and no added elevation. */
@Composable
fun PrimaryButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: ButtonColors =
        ButtonDefaults.buttonColors(
            containerColor = LocalPrimaryFill.current,
            contentColor = Color.White,
        ),
    content: @Composable RowScope.() -> Unit,
) {
    Button(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        colors = colors,
        elevation = null,
        content = content,
    )
}
