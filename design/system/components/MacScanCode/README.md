# MacScanCode

A code another device scans: one thing at a time. What to do above, the QR code, then the buttons that hand over the code as text (Copy code, Share…) and nothing else, then when it ends below. The code's text is never shown beside the QR code.

The QR code is drawn as large as its squares need for a phone's camera to read it: the more a code holds, the more squares it has and the larger it is drawn, and the sheet or dialog grows to fit it. Each square is 360/105 pt, inside a quiet zone four squares wide, and VoiceOver reads the whole square as the QR code. The boards draw a short code of 25 squares, 113 pt wide (a pairing or invite code without an S3 key); a code that carries an S3 key is drawn larger. `size` sets the picture's side on a printed page.

**In the code:** macOS: `MacScanCode` (`Views/Components/MacScanCode.swift`), the instruction, `QRCode` (`Views/Components/QRCode.swift`) at the side its squares need, and Copy code and Share…, which hand out the code to type; Add device's sheet and the invite's sheet draw it.
