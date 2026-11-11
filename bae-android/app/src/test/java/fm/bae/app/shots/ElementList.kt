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

    /** The list for the tree under [root] (the root has no semantics of its own and is not listed). */
    fun of(
        board: String,
        root: SemanticsNode,
    ): JSONObject {
        val density = root.layoutInfo.density.density
        val elements = org.json.JSONArray()

        fun visit(
            node: SemanticsNode,
            parent: Int?,
        ) {
            val index = elements.length()
            elements.put(element(node, parent, density))
            node.children.forEach { visit(it, index) }
        }
        root.children.forEach { visit(it, null) }
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

    private fun element(
        node: SemanticsNode,
        parent: Int?,
        density: Float,
    ): JSONObject {
        val config = node.config
        val role = config.getOrNull(SemanticsProperties.Role)
        val editable = config.getOrNull(SemanticsProperties.EditableText)
        val texts = config.getOrNull(SemanticsProperties.Text)
        val description = config.getOrNull(SemanticsProperties.ContentDescription)
        val kind = kind(config, role, editable != null, texts != null)
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
                .put("parent", orNull(parent))
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
