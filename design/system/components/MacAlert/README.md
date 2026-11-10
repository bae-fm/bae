# MacAlert

The title and message, left-aligned as macOS 26 draws them, with no icon. Two buttons sit side by side at equal width with the default on the right; three or more stack. List the action first and Cancel last. `destructive` on the action when it removes something. `warning` says what the choice costs, as a quiet warning notice between the title and the message ("Unsent changes will be lost"). `details` (an error's causes, one a line) adds a `MacErrorDetails` under the message: the Activity panel's problems open such an alert.

**In the code:** macOS: SwiftUI's `.alert`.
