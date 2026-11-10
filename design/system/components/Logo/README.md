# Logo

The logo in its forms: `pair`, the sheep over the word; `inline`, the sheep before the word, on the left, or on the right in a right-to-left language; `sheep`, the sheep alone; `word`, the word alone; `small`, the sheep drawn for 32px and under. `size` is the sheep's width. `bob` makes the sheep and the word each float, and `glow` puts a soft disc of light behind the sheep and a pool of light under it, with the word in `logo-word` (the wool's cream in dark mode, the outline's plum in light) and a thin edge at 45°, both for Welcome. The README's Logo section says where each goes.

**In the code:** macOS: `WelcomeLogo` (`Views/Welcome/WelcomeLogo.swift`), Welcome only. Android: `Logo` (`ui/components/Logo.kt`), on Welcome with the bob and the glow, and on the restore-over screen still.
