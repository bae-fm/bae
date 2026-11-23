package fm.bae.app.shots

import android.graphics.Paint
import androidx.compose.ui.geometry.Rect
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
import androidx.compose.ui.unit.toSize
import org.json.JSONObject
import kotlin.math.roundToInt

/**
 * The element list a shot writes beside its PNG as `<board>@android.json`: what Compose's merged
 * semantics tree holds, in the tree's order, in dp from the shot's top-left. This is the only
 * writer; every shot goes through [of]. A merged node of several texts (a clickable row of a title, a credit and a
 * duration) is listed as the one node TalkBack reads and taps, with no label of its own, and holds one text element
 * for each visible text the unmerged tree holds, since the merged node keeps only the first text's layout. A merged
 * node of one text (a clickable menu row) is that text, drawn where the unmerged tree's text node draws it.
 */
internal object ElementList {
    private const val PLATFORM = "android"
    private const val TENTH = 10.0
    private const val SLACK = 0.6
    private const val HEX_RADIX = 16
    private const val RGB_DIGITS = 8

    /**
     * The list for the tree under [root] (the root has no semantics of its own and is not listed), with the
     * [embedded] views' elements, which the tree does not hold, each placed at the bare node the tree keeps for
     * its view, or else where the reading order reaches its box: before the first element that starts lower, or
     * in its row, further right.
     */
    fun of(
        board: String,
        root: SemanticsNode,
        embedded: List<JSONObject> = emptyList(),
        unmerged: SemanticsNode = root,
    ): JSONObject {
        val density = root.layoutInfo.density.density
        val listed = mutableListOf<Listed>()

        // Compose gives a node with no traversal index the index 0.
        fun inTraversalOrder(nodes: List<SemanticsNode>) = nodes.sortedBy { it.config.getOrNull(SemanticsProperties.TraversalIndex) ?: 0f }

        fun visit(
            node: SemanticsNode,
            parent: Listed?,
        ) {
            val here = Listed(element(node, density, unmerged), parent)
            listed.add(here)
            mergedTexts(node, unmerged).forEach { listed.add(Listed(element(it, density, unmerged), here)) }
            // The list visits a node's children in traversal order, the order TalkBack reads them in, not the order
            // Compose draws them in.
            inTraversalOrder(node.children).forEach { visit(it, here) }
        }
        inTraversalOrder(root.children).forEach { visit(it, null) }
        // The tree keeps a bare, childless node where an embedded view sits. The element list puts the view's
        // elements at that node, in the view's order. When the tree has no such node, the list puts the view where
        // the reading order reaches its box.
        var from = 0
        val placed = mutableSetOf<Listed>()
        for (view in embedded) {
            val holder =
                listed
                    .filter { held -> held !in placed && holds(held.json, view) && listed.none { it.parent === held } }
                    .minByOrNull { area(it.json) }
            val here = Listed(view, null)
            val at =
                if (holder != null) {
                    var next = listed.indexOf(holder) + 1
                    while (next < listed.size && listed[next] in placed) next++
                    next
                } else {
                    (from until listed.size).firstOrNull { startsAfter(listed[it].json, view) } ?: listed.size
                }
            listed.add(at, here)
            placed.add(here)
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

    /** Whether [element] is a bare node (no words, no role) whose box holds [view]'s. */
    private fun holds(
        element: JSONObject,
        view: JSONObject,
    ): Boolean {
        if (element.getString("role") != "other" || !element.isNull("label")) return false
        val box = element.getJSONObject("bounds")
        val mine = view.getJSONObject("bounds")
        return box.getDouble("x") - mine.getDouble("x") <= SLACK &&
            box.getDouble("y") - mine.getDouble("y") <= SLACK &&
            mine.getDouble("x") + mine.getDouble("width") - (box.getDouble("x") + box.getDouble("width")) <= SLACK &&
            mine.getDouble("y") + mine.getDouble("height") - (box.getDouble("y") + box.getDouble("height")) <= SLACK
    }

    private fun area(element: JSONObject): Double {
        val box = element.getJSONObject("bounds")
        return box.getDouble("width") * box.getDouble("height")
    }

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
        unmerged: SemanticsNode,
    ): JSONObject {
        val config = node.config
        val role = config.getOrNull(SemanticsProperties.Role)
        val editable = config.getOrNull(SemanticsProperties.EditableText)
        val texts = config.getOrNull(SemanticsProperties.Text)
        val description = config.getOrNull(SemanticsProperties.ContentDescription)
        val merged = mergedTexts(node, unmerged)
        val ownText = texts != null && merged.isEmpty()
        val kind = kind(config, role, editable != null, ownText, description != null && node.children.isEmpty())
        val named = merged.isEmpty() || kind != "other"
        val drawn = if (kind == "text") drawnText(node, unmerged) else node
        // The box the node draws, not the 48 dp touch target Compose grows around a smaller one: a board draws the
        // box, and TouchTargetTest checks the touch target. It is the whole box, also where it reaches past the
        // shot's frame, as a board's box does.
        val bounds = if (kind == "text") textBox(drawn) else node.wholeBox()
        val out =
            JSONObject()
                .put("role", kind)
                .put("native", role?.toString() ?: native(editable != null, ownText, config))
                // TalkBack names a node by its content description when it has one, over the text it holds. A bare
                // node of several texts has the words of its children, which the board lists one by one with no
                // element around them.
                .put("label", orNull(description?.joinToString(" ") ?: texts?.takeIf { named }?.joinToString(" ") { it.text }))
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
        if (kind == "text") textStyle(drawn.config)?.let { out.put("text", it) }
        return out
    }

    /**
     * The visible texts a merged node of several texts holds, each as the unmerged tree's node of one text, in the
     * tree's order. Empty for a node of one text or none, and for a node whose unmerged texts do not read as its own.
     */
    private fun mergedTexts(
        node: SemanticsNode,
        unmerged: SemanticsNode,
    ): List<SemanticsNode> {
        val words = node.config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") { it.text } ?: return emptyList()
        val parts = textsUnder(find(unmerged, node.id) ?: node)
        return if (parts.size > 1 && parts.joinToString(" ") { it.first } == words) parts.map { it.second } else emptyList()
    }

    /**
     * The node that draws the text [node] reads: [node] itself, or for a merged node of one text (a clickable row
     * of a label) the unmerged tree's text node, whose box and layout are the text's, not the row's.
     */
    private fun drawnText(
        node: SemanticsNode,
        unmerged: SemanticsNode,
    ): SemanticsNode {
        val words = node.config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") { it.text } ?: return node
        val parts = textsUnder(find(unmerged, node.id) ?: node)
        return if (parts.size == 1 && parts.single().first == words) parts.single().second else node
    }

    /**
     * A text node's box as a browser reports a text's: vertically from the first line's ascent to the last line's
     * descent, inside the line boxes whose height only spaces the lines. Compose's node box is the line boxes.
     */
    private fun textBox(node: SemanticsNode): Rect {
        val box = node.wholeBox()
        val glyphs = glyphBox(node.config, box) ?: return box
        return Rect(box.left, glyphs.first, box.right, glyphs.second)
    }

    /**
     * The node's box from its position and size in the root. [SemanticsNode.boundsInRoot] clips the box to the root,
     * which cuts an element that reaches past the shot's frame.
     */
    private fun SemanticsNode.wholeBox(): Rect = if (layoutInfo.isPlaced) Rect(positionInRoot, size.toSize()) else Rect.Zero

    /** The node of [id] at or under [from]. */
    private fun find(
        from: SemanticsNode,
        id: Int,
    ): SemanticsNode? = if (from.id == id) from else from.children.firstNotNullOfOrNull { find(it, id) }

    /** The texts at or under [from] in the tree's order, each with the node that holds it. */
    private fun textsUnder(from: SemanticsNode): List<Pair<String, SemanticsNode>> {
        val own = from.config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") { it.text }
        val held = if (own == null) emptyList() else listOf(own to from)
        return held + from.children.flatMap(::textsUnder)
    }

    /** From the first line's ascent to the last line's descent of the text [config] holds, from the box [bounds]. */
    private fun glyphBox(
        config: SemanticsConfiguration,
        bounds: Rect,
    ): Pair<Float, Float>? {
        val top = bounds.top
        val results = mutableListOf<TextLayoutResult>()
        val read = config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action
        if (read == null || !read(results) || results.isEmpty()) return null
        val layout = results.first()
        val fontSize = layout.layoutInput.style.fontSize
        if (!fontSize.isSp) return null
        val size = with(layout.layoutInput.density) { fontSize.toPx() }
        val metrics = Paint().apply { textSize = size }.fontMetrics
        val first = top + layout.getLineBaseline(0) + metrics.ascent
        val last = top + layout.getLineBaseline(layout.lineCount - 1) + metrics.descent
        return first to last
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
