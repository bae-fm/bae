package fm.bae.app.shots

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.LayoutInfo
import androidx.compose.ui.node.ModifierNodeElement

/**
 * Marks a layout of a shot's scene as a window that the app opens over its screen: an open menu, a sheet or a dialog.
 * layoutlib draws none of those windows, so a scene draws their contents inside the screen. TalkBack moves its focus
 * into a window that opens, so [ElementList] lists the elements under a marked layout before the screen's own. The mark
 * adds no semantics, so it leaves Compose's reading chain as the app builds it.
 */
internal fun Modifier.windowOverScreen(): Modifier = this.then(WindowMark)

/** Whether this layout carries the [windowOverScreen] mark. */
internal fun LayoutInfo.isWindow(): Boolean = getModifierInfo().any { it.modifier === WindowMark }

private data object WindowMark : ModifierNodeElement<WindowMarkNode>() {
    override fun create() = WindowMarkNode()

    override fun update(node: WindowMarkNode) = Unit
}

private class WindowMarkNode : Modifier.Node()
