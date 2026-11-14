# AndroidPinField

The PIN an AirPlay receiver shows on its own screen, typed here: one outlined box per digit, the digits shown as typed, since the person reads them off another screen and wants to check them. The number of boxes is `digits`, which core states with the request (`Pin { digits }`), so the app holds no number of its own. While the field has focus, the box where the next digit goes has the accent outline. The keyboard is the number pad. It takes digits only and keeps leading zeros: 0420 is a different PIN from 420.

A wrong PIN draws every box in red and puts one line under them, in red. The field sits in the pairing dialog (`AndroidDialog`) beneath the dialog's message; a password receiver's dialog uses `AndroidTextField` with `secret` in its place.

**In the code:** Android: none yet; the pairing dialog needs a `PinField` taking the digit count, the typed text, and the error line.
