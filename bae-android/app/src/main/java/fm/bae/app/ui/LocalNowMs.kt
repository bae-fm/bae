package fm.bae.app.ui

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * The moment, in Unix epoch milliseconds, that the views which word a time against now read: the clock unless a
 * shot provides a fixed moment, so the words a shot shows do not depend on when it is taken.
 */
val LocalNowMs = staticCompositionLocalOf<() -> Long> { System::currentTimeMillis }
