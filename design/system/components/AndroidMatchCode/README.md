# AndroidMatchCode

The six digits two devices both show while one lets the other in: adding a device, joining from an invite, and the request the other side approves. Large, in two groups of three, centred, so the person can read it aloud and check it matches. `waiting` is the line under the digits that says what the device is waiting for, beside a spinner so it reads as still checking, ending in an ellipsis: “Waiting for your other device…”. `dialog` is the smaller size used inside a dialog, set a little further from the line above it so the code stands apart from the words that ask you to check it.

The same part draws the code on both sides of the match, so the two screens can't drift apart.

**In the code:** Android: `AndroidMatchCode` (`ui/components/AndroidMatchCode.kt`), the digits at a screen's size, or a dialog's with `dialog`, and `waiting` for the line under them.
