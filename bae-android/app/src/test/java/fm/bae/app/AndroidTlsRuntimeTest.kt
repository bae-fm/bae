package fm.bae.app

import org.junit.Assert.assertNotNull
import org.junit.Test

class AndroidTlsRuntimeTest {
    @Test
    fun platformVerifierJvmComponentShipsWithTheApp() {
        assertNotNull(Class.forName("org.rustls.platformverifier.CertificateVerifier"))
    }
}
