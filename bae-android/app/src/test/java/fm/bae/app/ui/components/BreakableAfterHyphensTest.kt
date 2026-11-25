package fm.bae.app.ui.components

import org.junit.Assert.assertEquals
import org.junit.Test

/** A subtitle is laid out so that its line can end after any of its hyphens, as the board's browser ends it. */
class BreakableAfterHyphensTest {
    @Test
    fun aLineMayEndAfterEachHyphen() {
        assertEquals("3b9e-​07d4-​51a2", "3b9e-07d4-51a2".breakableAfterHyphens())
    }

    @Test
    fun aTextWithoutHyphensIsUnchanged() {
        assertEquals("abc", "abc".breakableAfterHyphens())
    }
}
