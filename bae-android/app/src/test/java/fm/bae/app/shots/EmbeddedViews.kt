package fm.bae.app.shots

import android.view.View
import android.view.ViewGroup
import android.widget.ProgressBar
import android.widget.SeekBar
import android.widget.TextView
import org.json.JSONObject
import kotlin.math.roundToInt

/**
 * The elements of the native views a screen embeds with `AndroidView` (the seek row, the progress bars),
 * in the shape [ElementList] writes. Compose's semantics tree stops at an embedded view, which TalkBack
 * reads on through, so a list of the semantics alone would leave a seek bar's clocks and slider out.
 */
internal object EmbeddedViews {
    private const val TENTH = 10.0
    private const val PERCENT = 100.0
    private const val HEX_RADIX = 16
    private const val RGB_DIGITS = 8
    private const val COMPOSE_VIEW = "androidx.compose.ui.platform.AndroidComposeView"
    private const val HOLDER_SUFFIX = "ViewFactoryHolder"

    /** The elements under every embedded view below [window], in view order, with boxes in dp from the Compose view's top-left. */
    fun of(
        window: View,
        density: Float,
    ): List<JSONObject> {
        val compose = find(window) { it.javaClass.name == COMPOSE_VIEW } ?: return emptyList()
        val origin = IntArray(2).also { compose.getLocationInWindow(it) }
        val out = mutableListOf<JSONObject>()
        holders(compose).forEach { holder -> leaves(holder, origin, density, out) }
        return out
    }

    private fun find(
        view: View,
        match: (View) -> Boolean,
    ): View? {
        if (match(view)) return view
        if (view !is ViewGroup) return null
        for (index in 0 until view.childCount) {
            find(view.getChildAt(index), match)?.let { return it }
        }
        return null
    }

    /** The embedded views' holders below [view], outermost first, in the order Compose places them. */
    private fun holders(view: View): List<ViewGroup> {
        if (view !is ViewGroup) return emptyList()
        if (view.javaClass.name.endsWith(HOLDER_SUFFIX)) return listOf(view)
        return (0 until view.childCount).flatMap { holders(view.getChildAt(it)) }
    }

    private fun leaves(
        view: View,
        origin: IntArray,
        density: Float,
        out: MutableList<JSONObject>,
    ) {
        if (view.visibility != View.VISIBLE) return
        element(view, origin, density)?.let(out::add)
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) leaves(view.getChildAt(index), origin, density, out)
        }
    }

    private fun element(
        view: View,
        origin: IntArray,
        density: Float,
    ): JSONObject? {
        val label = view.contentDescription?.toString()
        val (kind, native, words) =
            when {
                view is SeekBar -> Triple("slider", "SeekBar", label)
                view is ProgressBar -> Triple("progress", "ProgressBar", label)
                view is TextView && view.text.isNotEmpty() -> Triple("text", "TextView", view.text.toString())
                else -> return null
            }
        val at = IntArray(2).also { view.getLocationInWindow(it) }
        val out =
            JSONObject()
                .put("role", kind)
                .put("native", native)
                .put("label", words ?: JSONObject.NULL)
                .put("value", percent(view) ?: JSONObject.NULL)
                .put(
                    "bounds",
                    JSONObject()
                        .put("x", dp(at[0] - origin[0], density))
                        .put("y", dp(at[1] - origin[1], density))
                        .put("width", dp(view.width, density))
                        .put("height", dp(view.height, density)),
                ).put("enabled", view.isEnabled)
                .put("selected", JSONObject.NULL)
                .put("parent", JSONObject.NULL)
        if (view is TextView) out.put("text", style(view))
        return out
    }

    /** How far a bar is, as a whole percent, the way the boards give a bar's value; null for a text or a moving bar. */
    private fun percent(view: View): String? =
        (view as? ProgressBar)
            ?.takeIf { !it.isIndeterminate && it.max > 0 }
            ?.let { (it.progress * PERCENT / it.max).roundToInt().toString() }

    /** The words' size in sp and colour, as the view draws them. */
    private fun style(view: TextView): JSONObject {
        val sp = view.textSize / view.resources.displayMetrics.scaledDensity
        val rrggbb = (view.currentTextColor and 0xFFFFFF).toString(HEX_RADIX).padStart(RGB_DIGITS - 2, '0')
        val alpha = ((view.currentTextColor ushr 24) and 0xFF).toString(HEX_RADIX).padStart(2, '0')
        return JSONObject()
            .put("size", (sp * TENTH).roundToInt() / TENTH)
            .put("color", "#" + (rrggbb + alpha).uppercase())
    }

    private fun dp(
        pixels: Int,
        density: Float,
    ): Double = (pixels / density * TENTH).roundToInt() / TENTH
}
