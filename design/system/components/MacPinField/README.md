# MacPinField

The PIN an AirPlay receiver shows on its own screen, typed here: one box per digit, the digits shown as typed, since the person reads them off another screen and wants to check them. The number of boxes is `digits`, which core states with the request (`Pin { digits }`), so the app holds no number of its own. While the field has focus, the box where the next digit goes is ringed in the accent. It takes digits only and keeps leading zeros: 0420 is a different PIN from 420.

A wrong PIN draws every box red and puts one line under them, in red. A field that waits for the receiver (`off`) is dimmed and takes nothing. The field sits in the pairing sheet (`MacSheet`) beneath the sheet's message; a password receiver's sheet uses `MacTextField` with `secret` in its place.

**In the code:** macOS: none yet; the pairing sheet needs a `PinField` taking the digit count, the typed text, and the error line.
