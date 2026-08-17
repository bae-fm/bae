using System;
using System.IO;
using Avalonia.Media.Imaging;
using QRCoder;

namespace Bae.Desktop;

/// <summary>
/// Renders a pairing or recovery code to a QR
/// <see cref="Bitmap"/> — the analog of macOS's <c>QRCode.image(from:)</c>. The
/// codes are scanned by another device's camera, so the image is just a visual
/// transport of the same text shown beside it.
///
/// QRCoder's <see cref="PngByteQRCode"/> produces a PNG byte array directly (no
/// System.Drawing), decoded into a <see cref="Bitmap"/> from an in-memory stream
/// the same way <see cref="ImageStore"/> decodes an image stream.
/// </summary>
public static class QrCode
{
    /// <summary>
    /// A QR image for <paramref name="text"/>, or null when it's empty or the encode
    /// fails (the caller falls back to the copyable text shown beside it).
    /// Error-correction level M, matching the other platforms.
    /// </summary>
    public static Bitmap? Image(string? text)
    {
        if (string.IsNullOrEmpty(text))
        {
            return null;
        }

        try
        {
            using var generator = new QRCodeGenerator();
            using var data = generator.CreateQrCode(text, QRCodeGenerator.ECCLevel.M);
            var png = new PngByteQRCode(data);
            // 10 px per module is crisp at the ~180 px the dialogs display it; the
            // image control scales it to fit.
            var bytes = png.GetGraphic(10);
            using var stream = new MemoryStream(bytes);
            return new Bitmap(stream);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Error("Failed to encode QR code.", exception);
            // An over-long payload (past QR's capacity) or an encoder failure returns
            // null; the caller falls back to the copyable text always shown beside it.
            return null;
        }
    }
}
