package fm.bae.app.ui.components

import androidx.compose.foundation.layout.RowScope
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/** A text button for what a dialog destroys: in the danger colour. */
@Composable
fun DestructiveTextButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    content: @Composable RowScope.() -> Unit,
) {
    TextButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
        content = content,
    )
}

/** A text button beside a destructive one (Cancel, Keep): in the screen's text colour. */
@Composable
fun NeutralTextButton(
    onClick: () -> Unit,
    content: @Composable RowScope.() -> Unit,
) {
    TextButton(
        onClick = onClick,
        colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurface),
        content = content,
    )
}
