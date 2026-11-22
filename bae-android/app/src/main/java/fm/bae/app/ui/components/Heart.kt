package fm.bae.app.ui.components

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.SymbolIcons

/**
 * The rating of a library whose rating system is Heart, read-only: one heart, filled in the accent when [on] (the
 * person gave three stars or more) and an outline otherwise. An outline heart says nothing to screen readers.
 */
@Composable
fun Heart(
    on: Boolean,
    size: Dp,
) {
    Icon(
        imageVector = if (on) SymbolIcons.Favorite else SymbolIcons.FavoriteBorder,
        contentDescription = if (on) stringResource(R.string.heart_on) else null,
        tint = if (on) MaterialTheme.colorScheme.primary else BaeTheme.colors.hairlineStrong,
        modifier = Modifier.size(size),
    )
}
