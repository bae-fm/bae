package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCreditJoin
import uniffi.bae_bridge.BridgeCreditLine
import uniffi.bae_bridge.BridgeCreditedName

/**
 * A credit line in the person's language: a recognized join is worded from the catalog, a join bae
 * does not recognize is shown as written, and a run of names with no join is a list.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class CreditLineTextTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    /** Core's `bridge_credit_join_key`, which a JVM test cannot call; the loc coverage test holds these keys. */
    private val joinKey: (BridgeCreditJoin) -> String? = {
        when (it) {
            BridgeCreditJoin.Featuring -> "core.credit.join.featuring"
            BridgeCreditJoin.Ampersand -> "core.credit.join.ampersand"
            BridgeCreditJoin.And -> "core.credit.join.and"
            BridgeCreditJoin.With -> "core.credit.join.with"
            BridgeCreditJoin.Versus -> "core.credit.join.versus"
            is BridgeCreditJoin.Unrecognized -> null
        }
    }

    private fun line(vararg names: Pair<String, BridgeCreditJoin?>) =
        BridgeCreditLine(names.map { (name, join) -> BridgeCreditedName(name, join) })

    @Test
    fun aRecognizedJoinIsWordedAndAPlainRunIsAList() {
        assertEquals(
            "Artist A feat. Artist B, Artist C, and Artist D",
            context.creditLineText(
                line(
                    "Artist A" to BridgeCreditJoin.Featuring,
                    "Artist B" to null,
                    "Artist C" to null,
                    "Artist D" to null,
                ),
                joinKey,
            ),
        )
    }

    @Test
    fun namesWithNoJoinAreJoinedByTheListFormatter() {
        assertEquals(
            "Artist A and Artist B",
            context.creditLineText(line("Artist A" to null, "Artist B" to null), joinKey),
        )
    }

    @Test
    fun aJoinBaeDoesNotRecognizeIsShownAsWritten() {
        assertEquals(
            "Artist A / Artist B",
            context.creditLineText(line("Artist A" to BridgeCreditJoin.Unrecognized(" / "), "Artist B" to null), joinKey),
        )
    }

    @Test
    fun everyRecognizedJoinHasItsOwnWording() {
        val joins =
            listOf(
                BridgeCreditJoin.Featuring to "Artist A feat. Artist B",
                BridgeCreditJoin.Ampersand to "Artist A & Artist B",
                BridgeCreditJoin.And to "Artist A and Artist B",
                BridgeCreditJoin.With to "Artist A with Artist B",
                BridgeCreditJoin.Versus to "Artist A vs. Artist B",
            )
        for ((join, expected) in joins) {
            assertEquals(expected, context.creditLineText(line("Artist A" to join, "Artist B" to null), joinKey))
        }
    }

    @Test
    fun oneNameIsItselfAndNoNamesAreEmpty() {
        assertEquals("Artist A", context.creditLineText(line("Artist A" to null), joinKey))
        assertEquals("", context.creditLineText(BridgeCreditLine(emptyList())))
    }
}
