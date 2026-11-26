package fm.bae.app.shots

import builds.shots.BoardShot
import fm.bae.app.BaeRobolectricTestRunner
import org.junit.runner.RunWith

/** A board's shot of bae: the real composable in [BaeShotLook], which each of bae's shots extends. */
@RunWith(BaeRobolectricTestRunner::class)
abstract class BaeBoardShot : BoardShot(BaeShotLook)
