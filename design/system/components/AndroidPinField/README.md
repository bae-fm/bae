# AndroidPinField

The person types the PIN that an AirPlay receiver shows on its own screen. The field has one outlined box per digit and shows each digit as the person types it, so they can check it against the receiver. The number of boxes is `digits`, which core states with the request (`Pin { digits }`), so the app holds no number of its own. While the field has focus, the box where the next digit goes has the accent outline. The keyboard is the number pad. The field takes digits only and keeps leading zeros: 0420 is a different PIN from 420.

When the PIN is wrong, the field draws every box in red and puts one line under them, in red. The field sits in the pairing dialog (`AndroidDialog`) beneath the dialog's message; a password receiver's dialog uses `AndroidTextField` with `secret` in its place.

**In the code:** Android: none yet; the pairing dialog needs a `PinField` taking the digit count, the typed text, and the error line.
