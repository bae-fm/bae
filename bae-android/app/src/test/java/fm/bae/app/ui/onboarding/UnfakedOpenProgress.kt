package fm.bae.app.ui.onboarding

import fm.bae.app.data.LibraryOpenProgress
import fm.bae.app.playback.unfaked

/** The open progress of a flow whose host is a fake, which runs no restore over this device's copy. */
internal fun unfakedOpenProgress(): LibraryOpenProgress = unfaked("LibraryOpenProgress")
