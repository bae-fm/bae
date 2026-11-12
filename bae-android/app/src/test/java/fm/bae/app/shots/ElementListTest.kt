package fm.bae.app.shots

import android.widget.LinearLayout
import android.widget.SeekBar
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.focused
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.github.takahirom.roborazzi.captureRoboImage
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.Description
import org.junit.runners.model.Statement
import java.io.File

/** The writer of every shot's element list, proved on one known screen. */
class ElementListTest : BoardShot() {
    @Test
    fun readsRolesWordsBoxesStylesParentsAndFocus() {
        compose.setContent {
            Column(modifier = Modifier.fillMaxSize()) {
                Box(
                    modifier =
                        Modifier
                            .offset(16.dp, 88.dp)
                            .semantics { contentDescription = "Group" },
                ) {
                    Text("People", color = Color(0xFF1B1B22), fontSize = 14.sp, fontWeight = FontWeight.Bold)
                }
                Button(onClick = {}) { Text("Invite") }
                Switch(checked = true, onCheckedChange = {}, modifier = Modifier.semantics { contentDescription = "Sync" })
                Box(
                    modifier =
                        Modifier.size(10.dp).semantics {
                            focused = true
                            contentDescription = "Focus"
                        },
                )
            }
        }
        val list = ElementList.of("android.test.known", compose.onRoot().fetchSemanticsNode())

        assertEquals("android.test.known", list.getString("board"))
        assertEquals("android", list.getString("platform"))
        assertEquals(2.625, list.getDouble("scale"), 0.0)
        assertEquals(411.0, list.getJSONObject("size").getDouble("width"), 1.0)
        assertEquals(914.0, list.getJSONObject("size").getDouble("height"), 1.0)
        val elements = list.getJSONArray("elements").objects()

        val group = elements.single { it.optString("label") == "Group" }
        assertEquals("other", group.getString("role"))
        assertTrue(group.isNull("parent"))
        val groupIndex = elements.indexOf(group)

        val people = elements.single { it.optString("label") == "People" }
        assertEquals("text", people.getString("role"))
        assertEquals(groupIndex, people.getInt("parent"))
        assertEquals(16.0, people.getJSONObject("bounds").getDouble("x"), 0.0)
        assertEquals(88.0, people.getJSONObject("bounds").getDouble("y"), 0.0)
        val style = people.getJSONObject("text")
        assertEquals(14.0, style.getDouble("size"), 0.0)
        assertEquals(700, style.getInt("weight"))
        assertEquals("#1B1B22FF", style.getString("color"))

        val invite = elements.single { it.optString("label") == "Invite" }
        assertEquals("button", invite.getString("role"))
        assertEquals("Button", invite.getString("native"))
        assertTrue(invite.getBoolean("enabled"))
        assertFalse(invite.has("text"))

        val toggle = elements.single { it.optString("label") == "Sync" }
        assertEquals("toggle", toggle.getString("role"))
        assertEquals("on", toggle.getString("value"))

        val focus = elements.single { it.optString("label") == "Focus" }
        assertTrue(focus.getBoolean("focused"))
        assertFalse(people.has("focused"))
    }

    @Test
    fun aLeafWithOnlyADescriptionIsAnImageUnlessClickable() {
        compose.setContent {
            Column(modifier = Modifier.fillMaxSize()) {
                Box(Modifier.size(10.dp).semantics { contentDescription = "Logo" })
                Box(Modifier.size(10.dp).clickable {}.semantics { contentDescription = "Tap" })
            }
        }
        val elements =
            ElementList
                .of("android.test.image", compose.onRoot().fetchSemanticsNode())
                .getJSONArray("elements")
                .objects()
        assertEquals("image", elements.single { it.optString("label") == "Logo" }.getString("role"))
        assertEquals("other", elements.single { it.optString("label") == "Tap" }.getString("role"))
    }

    @Test
    fun anEmbeddedViewsElementsComeWhereTheReadingOrderReachesThem() {
        compose.setContent {
            Column(modifier = Modifier.fillMaxSize()) {
                Text("Above")
                AndroidView(
                    factory = { context ->
                        LinearLayout(context).apply {
                            addView(TextView(context).apply { text = "1:42" })
                            addView(
                                SeekBar(context).apply {
                                    contentDescription = "Position"
                                    max = 100
                                    progress = 35
                                },
                            )
                        }
                    },
                )
                Text("Below")
            }
        }
        val embedded =
            EmbeddedViews.of(compose.activity.window.decorView, compose.activity.resources.displayMetrics.density)
        val elements =
            ElementList
                .of("android.test.embedded", compose.onRoot().fetchSemanticsNode(), embedded)
                .getJSONArray("elements")
                .objects()

        val named = elements.filterNot { it.isNull("label") }
        assertEquals(listOf("Above", "1:42", "Position", "Below"), named.map { it.getString("label") })
        assertEquals(listOf("text", "text", "slider", "text"), named.map { it.getString("role") })
        val slider = named.single { it.getString("label") == "Position" }
        assertTrue(slider.isNull("parent"))
        assertEquals("35", slider.getString("value"))
    }

    private fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }

    @Test
    fun aScreenWithAnEndlessAnimationIsDrawnTheSameEveryTime() {
        assertTrue(drawn().contentEquals(drawn()))
    }

    /** The PNG bytes of a screen with a spinner and an animation that never ends, drawn on a fresh rule. */
    private fun drawn(): ByteArray {
        val rule = createAndroidComposeRule<ComponentActivity>()
        var bytes = ByteArray(0)
        val run =
            object : Statement() {
                override fun evaluate() {
                    rule.settle {
                        val alpha by rememberInfiniteTransition(label = "endless")
                            .animateFloat(0f, 1f, infiniteRepeatable(tween(durationMillis = 1_000)), label = "alpha")
                        Column {
                            CircularProgressIndicator()
                            Box(Modifier.size(100.dp).graphicsLayer { this.alpha = alpha }.background(Color.Red))
                        }
                    }
                    val file = File.createTempFile("endless", ".png")
                    // The shots in this JVM record when -Pbae.shotsDir asked for them; put the property back.
                    val before = System.getProperty("roborazzi.test.record")
                    System.setProperty("roborazzi.test.record", "true")
                    try {
                        rule.onRoot().captureRoboImage(file.path)
                    } finally {
                        if (before ==
                            null
                        ) {
                            System.clearProperty("roborazzi.test.record")
                        } else {
                            System.setProperty("roborazzi.test.record", before)
                        }
                    }
                    bytes = file.readBytes()
                    check(bytes.isNotEmpty()) {
                        val recent =
                            File(System.getProperty("user.dir")!!)
                                .walkTopDown()
                                .maxDepth(8)
                                .filter { it.extension == "png" && System.currentTimeMillis() - it.lastModified() < 120_000 }
                                .take(10)
                                .toList()
                        "no PNG at ${file.path}; user.dir=${System.getProperty("user.dir")}; recent PNGs: $recent"
                    }
                    file.delete()
                }
            }
        rule.apply(run, Description.createTestDescription(ElementListTest::class.java, "drawn")).evaluate()
        return bytes
    }
}
