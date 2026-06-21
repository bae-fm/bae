using System;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using ZXing;
using ZXing.Common;

namespace Bae.Windows;

/// <summary>
/// Reads a QR code from an image the user picks (a screenshot or photo of the
/// other device's code) and returns the embedded string. The paste field beside
/// every scan entry point is the always-available fallback.
///
/// This scans from an image file rather than a live camera: WinUI 3 has no
/// <c>CaptureElement</c>, so a live preview would need a custom MediaCapture
/// frame pipeline. Picking an image of the code is a reliable scan, and the
/// paste field covers entry without an image.
/// </summary>
public static class QrScanner
{
    /// <summary>
    /// Let the user pick an image file, decode its QR, and return the embedded
    /// text — or null if they cancelled or no QR was found.
    /// <paramref name="windowHandle"/> is the owning window's HWND, needed to show
    /// the picker on an unpackaged app.
    /// </summary>
    public static async Task<string?> ScanFromFileAsync(IntPtr windowHandle)
    {
        var picker = new global::Windows.Storage.Pickers.FileOpenPicker
        {
            ViewMode = global::Windows.Storage.Pickers.PickerViewMode.Thumbnail,
            SuggestedStartLocation = global::Windows.Storage.Pickers.PickerLocationId.PicturesLibrary,
        };
        WinRT.Interop.InitializeWithWindow.Initialize(picker, windowHandle);
        picker.FileTypeFilter.Add(".png");
        picker.FileTypeFilter.Add(".jpg");
        picker.FileTypeFilter.Add(".jpeg");
        picker.FileTypeFilter.Add(".bmp");
        picker.FileTypeFilter.Add(".gif");

        var file = await picker.PickSingleFileAsync();
        if (file is null)
        {
            return null;
        }

        return await DecodeFileAsync(file);
    }

    /// <summary>
    /// Decode the QR in an image file to its embedded text, or null when the
    /// image can't be read or holds no QR.
    /// </summary>
    private static async Task<string?> DecodeFileAsync(global::Windows.Storage.StorageFile file)
    {
        try
        {
            using var stream = await file.OpenAsync(global::Windows.Storage.FileAccessMode.Read);
            var decoder = await global::Windows.Graphics.Imaging.BitmapDecoder.CreateAsync(stream);

            // Decode to BGRA8 straight pixels so the byte layout matches ZXing's
            // BGRA32 luminance source exactly (4 bytes/pixel, blue-green-red-alpha).
            using var bitmap = await decoder.GetSoftwareBitmapAsync(
                global::Windows.Graphics.Imaging.BitmapPixelFormat.Bgra8,
                global::Windows.Graphics.Imaging.BitmapAlphaMode.Straight);

            var width = bitmap.PixelWidth;
            var height = bitmap.PixelHeight;
            var pixels = new byte[width * height * 4];
            bitmap.CopyToBuffer(pixels.AsBuffer());

            var source = new RGBLuminanceSource(pixels, width, height, RGBLuminanceSource.BitmapFormat.BGRA32);
            var reader = new BarcodeReaderGeneric
            {
                Options = new DecodingOptions
                {
                    PossibleFormats = new[] { BarcodeFormat.QR_CODE },
                    TryHarder = true,
                },
            };
            var result = reader.Decode(source);
            return result?.Text;
        }
        catch (Exception)
        {
            // An unreadable or undecodable image returns null; the caller keeps
            // the paste field available.
            return null;
        }
    }
}
