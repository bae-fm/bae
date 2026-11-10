# MacStep

A window that walks through one step of joining or adding a device. It sits centred in the welcome window: a headline, an optional line under it in secondary text, then the step's own content (a field, a code) and its buttons in `MacStepActions`, a section apart. A button follows what it acts on rather than sitting at the window's foot. The same shape as Android's `AndroidStep`.

The welcome window has no title bar text, so the headline is the only place that says where you are.

A headline wraps balanced, so it never leaves one word alone on its last line, in any language (in the code: `NSParagraphStyle.lineBreakStrategy` `.pushOut` on the headline). A line under it wraps the same way.

**In the code:** macOS: none yet; the code needs one.
