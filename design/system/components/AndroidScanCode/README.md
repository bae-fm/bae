# AndroidScanCode

A code another device scans: one thing at a time. What to do above, the QR code, then the buttons that hand over the code as text (Copy code, Share) and nothing else, then when it ends below. The code's text is never shown beside the QR code.

The QR code is drawn as large as its squares need for a phone's camera to read it: the more a code holds, the more squares it has and the larger it is drawn, and the sheet or dialog grows to fit it. Each square, with the four-square margin around the code, is 360/105 dp, so the code's side follows what it holds. The boards draw a short code (a pairing or invite code without an S3 key) at 127 dp, as the app draws a pairing code; a code that carries an S3 key reaches 360 dp.

**In the code:** Android: `AndroidScanCode` (`ui/components/AndroidScanCode.kt`), the instruction, the QR code of what it is given, which `QRCodeImage` (`ui/components/QRCode.kt`) sizes by what it holds, and Copy code and Share, which hand out the code to type; Add device's dialog and the invite's dialog draw it.
