package fm.bae.app.ui.appearance

import androidx.compose.ui.text.TextStyle

internal fun TextStyle.withTabularDigits(): TextStyle = copy(fontFeatureSettings = "tnum")
