package fm.bae.app.shots

import androidx.compose.ui.graphics.isSpecified
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsConfiguration
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.text.TextLayoutResult
import org.json.JSONObject
import kotlin.math.roundToInt

/**
 * The element list a shot writes beside its PNG as `<board>@android.json`: what Compose's merged
 * semantics tree holds, in the tree's order, in dp from the shot's top-left. This is the only
 * writer; every shot goes through [of].
 */
internal object ElementList {
    private const val PLATFORM = "android"
    private const val TENTH = 10.0
    private const val HEX_RADIX = 16
    private const val RGB_DIGITS = 8

    /**
     * The list for the tree under [root] (the root has no semantics of its own and is not listed), with the
     * [embedded] views' elements, which the tree does not hold, each placed where the reading order reaches
     * its box: before the first element that starts lower, or in its row, further right.
     */
    fun of(
        board: String,
        root: SemanticsNode,
        embedded: List<JSONObject> = emptyList(),
    ): JSONObject {
        val density = root.layoutInfo.density.density
        val listed = mutableListOf<Listed>()

        fun visit(
            node: SemanticsNode,
            parent: Listed?,
        ) {
            val here = Listed(element(node, density), parent)
            listed.add(here)
            node.children.forEach { visit(it, here) }
        }
        root.children.forEach { visit(it, null) }
        var from = 0
        for (view in embedded) {
            val at = (from until listed.size).firstOrNull { startsAfter(listed[it].json, view) } ?: listed.size
            listed.add(at, Listed(view, null))
            from = at + 1
        }
        val elements = org.json.JSONArray()
        listed.forEach { it.json.put("parent", it.parent?.let(listed::indexOf) ?: JSONObject.NULL) }
        listed.forEach { elements.put(it.json) }
        return JSONObject()
            .put("board", board)
            .put("platform", PLATFORM)
            .put(
                "size",
                JSONObject()
                    .put("width", dp(root.size.width, density))
                    .put("height", dp(root.size.height, density)),
            ).put("scale", density.toDouble())
            .put("elements", elements)
    }

    /** An element of the tree and the one that holds it. */
    private class Listed(
        val json: JSONObject,
        val parent: Listed?,
    )

    /** Whether [element] starts lower than [view]'s box, or in its row and further right. */
    private fun startsAfter(
        element: JSONObject,
        view: JSONObject,
    ): Boolean {
        val box = element.getJSONObject("bounds")
        val mine = view.getJSONObject("bounds")
        val lower = box.getDouble("y") - mine.getDouble("y")
        return lower > mine.getDouble("height") / 2 ||
            (Math.abs(lower) <= mine.getDouble("height") / 2 && box.getDouble("x") > mine.getDouble("x"))
    }

    private fun element(
        node: SemanticsNode,
        density: Float,
    ): JSONObject {
        val config = node.config
        val role = config.getOrNull(SemanticsProperties.Role)
        val editable = config.getOrNull(SemanticsProperties.EditableText)
        val texts = config.getOrNull(SemanticsProperties.Text)
        val description = config.getOrNull(SemanticsProperties.ContentDescription)
        val kind = kind(config, role, editable != null, texts != null, description != null && node.children.isEmpty())
        val bounds = node.boundsInRoot
        val out =
            JSONObject()
                .put("role", kind)
                .put("native", role?.toString() ?: native(editable != null, texts != null, config))
                .put("label", orNull(texts?.joinToString(" ") { it.text } ?: description?.joinToString(" ")))
                .put("value", orNull(value(config, editable?.text)))
                .put(
                    "bounds",
                    JSONObject()
                        .put("x", dp(bounds.left, density))
                        .put("y", dp(bounds.top, density))
                        .put("width", dp(bounds.width, density))
                        .put("height", dp(bounds.height, density)),
                ).put("enabled", !config.contains(SemanticsProperties.Disabled))
                .put("selected", orNull(config.getOrNull(SemanticsProperties.Selected)))
        config.getOrNull(SemanticsProperties.Focused)?.let { out.put("focused", it) }
        if (kind == "text") textStyle(config)?.let { out.put("text", it) }
        return out
    }

    /** JSON null for an absent value: [JSONObject.put] would drop the key. */
    private fun orNull(value: Any?): Any = value ?: JSONObject.NULL

    private fun kind(
        config: SemanticsConfiguration,
        role: Role?,
        editable: Boolean,
        text: Boolean,
        describedLeaf: Boolean,
    ): String =
        when {
            role == Role.Button -> {
                "button"
            }

            role == Role.Checkbox || role == Role.Switch -> {
                "toggle"
            }

            role == Role.RadioButton -> {
                "radio"
            }

            role == Role.Tab -> {
                "tab"
            }

            role == Role.Image -> {
                "image"
            }

            role == Role.DropdownList -> {
                "popup"
            }

            editable -> {
                "field"
            }

            config.contains(SemanticsProperties.ProgressBarRangeInfo) -> {
                if (config.contains(SemanticsActions.SetProgress)) "slider" else "progress"
            }

            text -> {
                "text"
            }

            // Compose gives an Image no role: a leaf that only has a content description, no words and no
            // click, is how a picture (a logo, a cover) reaches the tree.
            describedLeaf && !config.contains(SemanticsActions.OnClick) -> {
                "image"
            }

            else -> {
                "other"
            }
        }

    private fun native(
        editable: Boolean,
        text: Boolean,
        config: SemanticsConfiguration,
    ): String =
        when {
            editable -> "EditableText"
            config.contains(SemanticsProperties.ProgressBarRangeInfo) -> "ProgressBarRangeInfo"
            text -> "Text"
            else -> "Node"
        }

    private fun value(
        config: SemanticsConfiguration,
        field: CharSequence?,
    ): Any? =
        field?.toString()
            ?: when (config.getOrNull(SemanticsProperties.ToggleableState)) {
                ToggleableState.On -> "on"
                ToggleableState.Off -> "off"
                else -> null
            }

    /** The text's size in sp, weight on the 100 to 900 scale, and colour, from the layout it was drawn with. */
    private fun textStyle(config: SemanticsConfiguration): JSONObject? {
        val results = mutableListOf<TextLayoutResult>()
        val read = config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action
        if (read == null || !read(results) || results.isEmpty()) return null
        val style = results.first().layoutInput.style
        val out = JSONObject()
        if (style.fontSize.isSp) out.put("size", style.fontSize.value.toDouble())
        style.fontWeight?.let { out.put("weight", it.weight) }
        if (style.color.isSpecified) out.put("color", hex(style.color.toArgb()))
        return out.takeIf { it.length() > 0 }
    }

    /** sRGB as `#RRGGBBAA` from an ARGB int. */
    private fun hex(argb: Int): String {
        val rrggbb = (argb and 0xFFFFFF).toString(HEX_RADIX).padStart(RGB_DIGITS - 2, '0')
        val alpha = ((argb ushr 24) and 0xFF).toString(HEX_RADIX).padStart(2, '0')
        return "#" + (rrggbb + alpha).uppercase()
    }

    private fun dp(
        pixels: Int,
        density: Float,
    ): Double = dp(pixels.toFloat(), density)

    private fun dp(
        pixels: Float,
        density: Float,
    ): Double = (pixels / density * TENTH).roundToInt() / TENTH
}
