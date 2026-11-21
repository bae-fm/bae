package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.coreString
import fm.bae.app.playback.SidePausePrompt
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.playback.sidePauseParts
import org.junit.Test
import java.time.Instant

/** The component shots of `AndroidSidePause`, one for each row of its preview. */
class AndroidSidePauseShots : BoardShot() {
    @Test
    fun androidComponentSidePauseCountingDown() =
        component(
            "android.component.side-pause.counting-down",
        ) { SidePause(PreviewData.sidePausePrompt(Instant.ofEpochSecond(12))) }

    @Test
    fun androidComponentSidePausePaused() = component("android.component.side-pause.paused") { SidePause(side()) }

    @Test
    fun androidComponentSidePauseBoxCleared() =
        component("android.component.side-pause.box-cleared") { SidePause(side(), keepPausing = false) }

    @Test
    fun androidComponentSidePauseDisc() =
        component("android.component.side-pause.disc") { SidePause(PreviewData.discPausePrompt(Instant.ofEpochSecond(12))) }
}

private fun side() = PreviewData.sidePausePrompt()

/** The side-pause dialog for [wording], with its title and its countdown, when it has one. */
@Composable
private fun SidePause(
    wording: SidePausePrompt,
    keepPausing: Boolean = true,
) {
    val context = LocalContext.current
    DialogInCell(
        sidePauseParts(
            title = wording.title(context),
            countdown = wording.prompt.countdown,
            nowMs = { 0L },
            keepPausingLabel = context.coreString(wording.keepPausingKey),
            keepPausing = keepPausing,
            onKeepPausingChange = {},
            onPlay = {},
            onClose = {},
        ),
    )
}
