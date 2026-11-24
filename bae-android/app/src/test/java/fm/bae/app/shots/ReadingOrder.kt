package fm.bae.app.shots

import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.platform.ViewRootForTest
import androidx.compose.ui.semantics.SemanticsNode
import androidx.test.core.app.ApplicationProvider
import org.robolectric.Shadows

/**
 * The order TalkBack reads a Compose screen's nodes in, from Compose's own accessibility delegate. The node provider
 * gives TalkBack no sorted child list: the delegate sorts all nodes by place on screen and traversal index into one
 * chain, and tells each node the node that follows it. [ElementList] lists each node's children by their place in
 * that chain.
 */
internal fun interface ReadingOrder {
    /** The semantics node ids under [parent], in reading order, or null when the provider has no such node. */
    fun childrenOf(parent: Int): List<Int>?

    companion object {
        private const val COMPOSE_VIEW = "androidx.compose.ui.platform.AndroidComposeView"
        private const val ID_SHIFT = 32
        private const val NONE = -1
        private const val BEFORE_KEY = "android.view.accessibility.extra.EXTRA_DATA_TEST_TRAVERSALBEFORE_VAL"
        private const val AFTER_KEY = "android.view.accessibility.extra.EXTRA_DATA_TEST_TRAVERSALAFTER_VAL"

        /** The id of the provider's host node, whose children are the children of the semantics root. */
        const val HOST = -1

        /** The tree order, for a test of the list that has no view. */
        val None = ReadingOrder { null }

        /**
         * Turns on Compose's accessibility sorting for the screen that [root] belongs to, as an enabled TalkBack does.
         * Compose sorts only while an accessibility service runs.
         */
        fun enableTalkBack(root: SemanticsNode) {
            // Compose sends accessibility events while it is forced on, which the platform's manager refuses when off.
            Shadows
                .shadowOf(
                    ApplicationProvider.getApplicationContext<Context>().getSystemService(
                        Context.ACCESSIBILITY_SERVICE,
                    ) as AccessibilityManager,
                ).setEnabled(true)
            (root.root as ViewRootForTest).forceAccessibilityForTesting(true)
        }

        /** The reading order of the Compose view below [window], after [enableTalkBack]. */
        fun of(window: View): ReadingOrder {
            val provider = checkNotNull(find(window)) { "no Compose view below the window" }.accessibilityNodeProvider
            val childId = AccessibilityNodeInfo::class.java.getMethod("getChildId", Int::class.java)

            fun virtual(id: Long) = (id ushr ID_SHIFT).toInt()
            val children = mutableMapOf<Int, List<Int>>()
            val next = mutableMapOf<Int, Int>()

            fun read(id: Int) {
                if (id in children) return
                val node = provider.createAccessibilityNodeInfo(id) ?: return
                children[id] = (0 until node.childCount).map { virtual(childId.invoke(node, it) as Long) }
                children.getValue(id).forEach(::read)
            }
            read(HOST)
            val view = checkNotNull(find(window))
            val delegate =
                view.javaClass
                    .getDeclaredField("composeAccessibilityDelegate")
                    .apply { isAccessible = true }
                    .get(view)!!
            // The node ids of Compose's chain: the delegate's map from each node to the node read after it.
            val map =
                delegate.javaClass
                    .getDeclaredField("idToBeforeMap")
                    .apply { isAccessible = true }
                    .get(delegate)!!
            val lookup = map.javaClass.getMethod("getOrDefault", Int::class.java, Int::class.java)
            for (id in children.keys) (lookup.invoke(map, id, NONE) as Int).takeIf { it != NONE }?.let { next[id] = it }
            val rank = mutableMapOf<Int, Int>()
            for (head in children.keys.filter { it !in next.values && it in next.keys }) {
                var at: Int? = head
                while (at != null && at !in rank) {
                    rank[at] = rank.size
                    at = next[at]
                }
            }
            return ReadingOrder { parent -> children[parent]?.sortedBy { rank[it] ?: Int.MAX_VALUE } }
        }

        private fun find(view: View): View? =
            if (view.javaClass.name == COMPOSE_VIEW) {
                view
            } else {
                (view as? ViewGroup)?.let { group -> (0 until group.childCount).firstNotNullOfOrNull { find(group.getChildAt(it)) } }
            }
    }
}
