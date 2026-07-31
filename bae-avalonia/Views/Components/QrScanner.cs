using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Platform.Storage;
using SkiaSharp;
using ZXing;
using ZXing.Common;

namespace Bae.Desktop;

/// <summary>
/// Reads a QR code from an image the user picks (a screenshot or photo of the
/// other device's code) and returns the embedded string. The paste field beside
/// every scan entry point is the always-available fallback.
///
/// This scans from an image file rather than a live camera: picking an image of
/// the code is a reliable read, and the paste field covers entry without an
/// image.
/// </summary>
public static class QrScanner
{
    private static readonly FilePickerFileType ImageFiles = new("Images")
    {
        Patterns = new[] { "*.png", "*.jpg", "*.jpeg", "*.bmp", "*.gif", "*.webp" },
        AppleUniformTypeIdentifiers = new[] { "public.image" },
        MimeTypes = new[] { "image/*" },
    };

    /// <summary>
    /// Let the user pick an image file, decode its QR, and return the embedded
    /// text — or null if they cancelled or no QR was found. <paramref name="anchor"/>
    /// is any control in the window; its top level owns the file picker.
    /// </summary>
    public static async Task<string?> ScanFromFileAsync(Visual anchor)
    {
        var storage = TopLevel.GetTopLevel(anchor)?.StorageProvider;
        if (storage is null)
        {
            return null;
        }

        var files = await storage.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            AllowMultiple = false,
            FileTypeFilter = new List<FilePickerFileType> { ImageFiles },
        });
        if (files.Count == 0)
        {
            return null;
        }

        await using var stream = await files[0].OpenReadAsync();
        return await DecodeAsync(stream);
    }

    /// <summary>
    /// Decode the QR in an image stream to its embedded text, or null when the
    /// image can't be read or holds no QR.
    /// </summary>
    private static async Task<string?> DecodeAsync(Stream stream)
    {
        try
        {
            // Buffer to memory so Skia's codec can seek, then decode straight into
            // BGRA8888 so the byte layout matches ZXing's BGRA32 luminance source
            // exactly (4 bytes/pixel, blue-green-red-alpha, unpremultiplied).
            using var buffer = new MemoryStream();
            await stream.CopyToAsync(buffer);
            buffer.Position = 0;

            using var codec = SKCodec.Create(buffer);
            if (codec is null)
            {
                return null;
            }

            var info = new SKImageInfo(
                codec.Info.Width,
                codec.Info.Height,
                SKColorType.Bgra8888,
                SKAlphaType.Unpremul);
            using var bitmap = new SKBitmap(info);
            var read = codec.GetPixels(info, bitmap.GetPixels());
            if (read is not (SKCodecResult.Success or SKCodecResult.IncompleteInput))
            {
                return null;
            }

            var source = new RGBLuminanceSource(
                bitmap.Bytes,
                info.Width,
                info.Height,
                RGBLuminanceSource.BitmapFormat.BGRA32);
            var reader = new BarcodeReaderGeneric
            {
                Options = new DecodingOptions
                {
                    PossibleFormats = new[] { BarcodeFormat.QR_CODE },
                    TryHarder = true,
                },
            };
            return reader.Decode(source)?.Text;
        }
        catch (Exception exception)
        {
            // An unreadable or undecodable image returns null; the caller keeps
            // the paste field available.
            BaeDiagnostics.Logger.Error("Failed to decode QR image.", exception);
            return null;
        }
    }
}
