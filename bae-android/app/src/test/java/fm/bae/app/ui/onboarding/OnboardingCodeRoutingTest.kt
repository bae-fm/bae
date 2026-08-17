package fm.bae.app.ui.onboarding

import org.junit.Assert.assertEquals
import org.junit.Test

class OnboardingCodeRoutingTest {
    @Test
    fun pairingEnvelopeFromLandingScannerOpensJoinFlow() {
        val pairingCodes = mutableListOf<String>()
        val restoreCodes = mutableListOf<String>()

        routeScannedSetupCode(
            code = "pairing-code",
            isPairingCode = true,
            onPairingCode = { pairingCodes.add(it) },
            onRestoreCode = { restoreCodes.add(it) },
        )

        assertEquals(listOf("pairing-code"), pairingCodes)
        assertEquals(emptyList<String>(), restoreCodes)
    }

    @Test
    fun restoreEnvelopeFromLandingScannerOpensRestoreFlow() {
        val pairingCodes = mutableListOf<String>()
        val restoreCodes = mutableListOf<String>()

        routeScannedSetupCode(
            code = "restore-code",
            isPairingCode = false,
            onPairingCode = { pairingCodes.add(it) },
            onRestoreCode = { restoreCodes.add(it) },
        )

        assertEquals(emptyList<String>(), pairingCodes)
        assertEquals(listOf("restore-code"), restoreCodes)
    }
}
