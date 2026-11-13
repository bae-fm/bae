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

    @Test
    fun anInstrumentOnMusicBrainzsListIsWordedFromTheCatalog() {
        val role =
            BridgeCreditRole(
                BridgeTermLabel.Localized("core.credit.instrument.17f9f065-2312-4a24-8309-6f6dd63e2e33"),
                listOf(BridgeTermLabel.Localized("core.credit.qualifier.guest")),
            )

        assertEquals("Bass guitar (guest)", context.creditRoleText(role))
    }

    @Test
    @Config(qualifiers = "de")
    fun anInstrumentIsWordedInThePersonsLanguage() {
        val role =
            BridgeCreditRole(
                BridgeTermLabel.Localized("core.credit.instrument.17f9f065-2312-4a24-8309-6f6dd63e2e33"),
                emptyList(),
            )
        val voice =
            BridgeCreditRole(
                BridgeTermLabel.Localized("core.credit.voice.8e2a3255-87c2-4809-a174-98cb3704f1a5"),
                emptyList(),
            )

        assertEquals(DE_BASS_GUITAR, context.creditRoleText(role))
        assertEquals("Hauptgesang", context.creditRoleText(voice))
    }

    private companion object {
        const val DE_BASS_GUITAR = "Bassgitarre"
    }
}
