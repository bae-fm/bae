# AndroidStep

A screen that walks through one step of joining or adding a device. It sits centred in the space above the keyboard (or the whole screen when the keyboard is down), with its text centred: a headline that says what to do or what happened, an optional line under it in secondary text, then the step's own content (a field, a code) and its buttons, a section apart. A button follows what it acts on rather than sitting at the bottom of the screen.

A step to read or fill in (your name, an invite or code that ended) is `top`: it sits at the top of the screen, left-aligned, as a page or form reads, with its button right under it. A step whose content is one thing to look at, a code, stays centred.

A question goes in the field's helper line, not a sentence above it.

The headline wraps balanced, so it never leaves one word alone on its last line, in any language (the code should set Compose's `LineBreak.Heading` on the headline and `LineBreak.Paragraph` on the line under it).

**In the code:** Android: `AndroidStep` (`ui/components/AndroidStep.kt`), in its `top` form only; the centred form comes with the join's waiting step.
