package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeTermLabel

/**
 * A credit's role in the person's language: a role bae names is worded from the catalog, the catalog's own
 * words are shown as written, and qualifiers follow in parentheses as a list.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class CreditRoleTextTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    @Test
    fun aRoleOfOneWordIsThatWord() {
        val role = BridgeCreditRole(BridgeTermLabel.Verbatim("Arranger"), emptyList())

        assertEquals("Arranger", context.creditRoleText(role))
    }

    @Test
    fun qualifiersFollowTheRoleInParenthesesAsAList() {
        val role =
            BridgeCreditRole(
                BridgeTermLabel.Verbatim("Engineer"),
                listOf(
                    BridgeTermLabel.Verbatim("assistant"),
                    BridgeTermLabel.Verbatim("Strings"),
                    BridgeTermLabel.Verbatim("Album Design"),
                ),
            )

        assertEquals("Engineer (assistant, Strings, and Album Design)", context.creditRoleText(role))
    }

    @Test
    fun aRoleBaeNamesIsWordedFromTheCatalog() {
        val role =
            BridgeCreditRole(
                BridgeTermLabel.Localized("core.credit.role.engineer"),
                listOf(BridgeTermLabel.Localized("core.credit.qualifier.assistant")),
            )

        assertEquals("Engineer (assistant)", context.creditRoleText(role))
    }
}
