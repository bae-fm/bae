package fm.bae.app.ui

import android.view.View
import android.view.ViewGroup
import android.widget.TextView

/** The text of every TextView under this View that is shown: what a View that draws a fast value put on screen. */
internal fun View.shownTexts(): List<String> =
    when (this) {
        is TextView -> if (isShown) listOf(text.toString()) else emptyList()
        is ViewGroup -> (0 until childCount).flatMap { getChildAt(it).shownTexts() }
        else -> emptyList()
    }
