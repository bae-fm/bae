# MacPinField

The person types the PIN that an AirPlay receiver shows on its own screen. The field has one box per digit and shows each digit as the person types it, so they can check it against the receiver. The number of boxes is `digits`, which core states with the request (`Pin { digits }`), so the app holds no number of its own. While it has focus, the field rings the box where the next digit goes in the accent. The field takes digits only and keeps leading zeros: 0420 is a different PIN from 420.

When the PIN is wrong, the field draws every box red and puts one line under them, in red. While the field waits for the receiver (`off`), it dims and takes nothing. The field sits in the pairing sheet (`MacSheet`) beneath the sheet's message; a password receiver's sheet uses `MacTextField` with `secret` in its place.

**In the code:** macOS: none yet; the pairing sheet needs a `PinField` taking the digit count, the typed text, and the error line.
