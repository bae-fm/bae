using System;
using System.IO;
using Microsoft.UI.Xaml.Media.Imaging;
using QRCoder;

namespace Bae.Windows;

/// <summary>
/// Renders a string (a join-request, invite, or recovery code) to a QR
/// <see cref="BitmapImage"/> for a WinUI image control — the analog of macOS's
/// <c>QRCode.image(from:)</c>. The codes are scanned by another device's camera,
/// so the image is just a visual transport of the same text shown beside it.
///
/// QRCoder's <see cref="PngByteQRCode"/> produces a PNG byte array directly, so
/// there's no System.Drawing dependency; the bytes are decoded into a
/// <c>BitmapImage</c> from an in-memory stream the same way <see cref="CoverImage"/>
/// decodes a file stream.
/// </summary>
public static class QrCode
{
    /// <summary>
    /// A QR image for <paramref name="text"/>, or null when it's empty or the
    /// encode fails (logged by the caller's null handling). Error-correction level
    /// M, matching the other platforms.
    /// </summary>
    public static BitmapImage? Image(string? text)
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
            // 10 px per module gives a crisp image at the ~180-200 px the dialogs
            // display it; the Image control scales it to fit.
            var bytes = png.GetGraphic(10);

            using var stream = new MemoryStream(bytes);
            var bitmap = new BitmapImage();
            bitmap.SetSource(stream.AsRandomAccessStream());
            return bitmap;
        }
        catch (Exception)
        {
            // An over-long payload (past QR's capacity) or an encoder failure
            // returns null; the caller falls back to the copyable text, which is
            // always shown beside the QR.
            return null;
        }
    }
}
