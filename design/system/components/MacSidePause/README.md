# MacSidePause

The pause between sides or discs: a card over the window, in the face every Mac sheet shares, as `MacSheet` draws it. Title, "Playback is paused. Press Play to continue.", the countdown when one is set, the checkbox (checked; unchecked adds where to turn it back on), Close and Play.

**In the code:** macOS: BaeKit's `MacSidePause` (`BaeKit/Sources/BaeKit/Views/MacSidePause.swift`) draws the card from the prompt and the checkbox's state, which it takes as a binding. `MacSidePauseContainer`, in the same file, holds that state, reads the prompt from the playback store, and shows the card over the window; `.sidePausePromptAlert` applies it.
