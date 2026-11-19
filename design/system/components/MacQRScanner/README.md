# MacQRScanner

The camera's picture in the Scan a code sheet, where this Mac reads a code another device shows: 300pt tall, with a card's corners, inside a `MacSheet` titled "Scan a code" over the screen whose paste field stays the other way in. Until the picture shows, the area says why in secondary text on the well.

| `state` | What the area shows |
|---|---|
| `starting` | "Starting camera…" while the camera opens. The default. |
| `denied` | "Camera access is off. Enable it in System Settings, or paste the code below." |
| `unavailable` | "No camera available. Paste the code below instead." |
| `camera` | The camera's picture, which bae reads the code from. |

**In the code:** macOS: `QRScannerView` (`Views/Settings/Library/QRScannerView.swift`), framed by `CodeScannerSheet` there.
