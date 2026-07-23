#if DEBUG
using System.Collections.Generic;
using System.IO;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Static fixtures for the debug component gallery — the Windows analogue of the
// macOS PreviewData set. Generic placeholder values only: no real
// artist/album/song names, no bridge, keychain, or library access. Compiled
// only in DEBUG builds, alongside the gallery it feeds.
internal static class PreviewData
{
    // Fixture libraries for the welcome chooser: two on-disk libraries the
    // welcome scene lists as re-openable, plus the create/restore actions. Local
    // only (no cloud provider), inactive, no open error — placeholder ids/names,
    // no keychain or on-disk library behind them.
    internal static List<BridgeLibrary> WelcomeLibraries { get; } = new()
    {
        new BridgeLibrary("lib-home", "Home Library", @"C:\Users\Example\Music\Home", null, false, null),
        new BridgeLibrary("lib-studio", "Studio Library", @"C:\Users\Example\Music\Studio", null, false, null),
    };

    // A signals row covering the render states SignalBadgeRow.Build draws: a
    // matched disc-id, an in-flight barcode lookup, an unmatched catalog number,
    // and an excluded (struck-through) badge.
    internal static IReadOnlyList<SignalBadge> SignalBadges { get; } = new[]
    {
        new SignalBadge
        {
            Kind = "disc_id",
            Value = "a1b2c3d4e5f6",
            State = new SignalBadgeState { Kind = "found", Count = 3 },
        },
        new SignalBadge
        {
            Kind = "barcode",
            Value = "0123456789012",
            State = new SignalBadgeState { Kind = "looking_up" },
        },
        new SignalBadge
        {
            Kind = "catalog",
            Value = "CAT-0001",
            State = new SignalBadgeState { Kind = "no_match" },
        },
        new SignalBadge
        {
            Kind = "barcode",
            Value = "9876543210987",
            State = new SignalBadgeState { Kind = "confirms", Count = 1 },
            Excluded = true,
        },
    };

    // A placeholder share/recovery code for the code-display block: a QR image
    // plus the code as monospaced text. Not a real code.
    internal static string SampleCode { get; } = "BAE-DEMO-0000-1111-2222-3333";

    // Fixture album tiles for the library-grid scene and the card gallery entry:
    // generic placeholder names and solid-color placeholder covers generated in
    // memory (no files, no library handle). Built fresh on each access so the
    // bitmaps are created on the calling UI thread, not at static init.
    internal static IReadOnlyList<(string Title, string Artist, string Year, ImageSource? Cover)> GridCards =>
        new (string Title, string Artist, string Year, ImageSource? Cover)[]
        {
            ("Album Title One", "Artist One", "2021", SolidCover(0x5B, 0x8F, 0xB0)),
            ("Album Title Two", "Artist Two", "2019", SolidCover(0xB0, 0x6A, 0x5B)),
            ("Album Title Three", "Artist Three", "2020", SolidCover(0x6F, 0xA0, 0x76)),
            ("Album Title Four", "Artist Four", "2017", SolidCover(0x8C, 0x7A, 0xB0)),
            ("Album Title Five", "Artist Five", "2022", SolidCover(0xB0, 0x9C, 0x5B)),
            ("Album Title Six", "Artist Six", "2016", SolidCover(0x5B, 0xB0, 0xA6)),
            ("Album Title Seven", "Artist Seven", "2018", SolidCover(0xA8, 0x5B, 0x8F)),
            ("Album Title Eight", "Artist Eight", "2023", SolidCover(0x70, 0x84, 0xA0)),
            ("Album Title Nine", "Artist Nine", "2015", SolidCover(0xA0, 0x88, 0x70)),
            ("Album Title Ten", "Artist Ten", "2021", SolidCover(0x84, 0xA0, 0x70)),
            ("Album Title Eleven", "Artist Eleven", "2019", SolidCover(0x9A, 0x70, 0xA0)),
            ("Album Title Twelve", "Artist Twelve", "2020", SolidCover(0x70, 0xA0, 0x9A)),
        };

    // A solid-color placeholder cover: a small 24-bit BMP composed in memory
    // (pure byte math — no WinRT buffer APIs) and decoded through the same
    // MemoryStream → AsRandomAccessStream → BitmapImage.SetSource path CoverImage
    // uses, standing in for real cover art with no file or handle behind it.
    private static BitmapImage SolidCover(byte r, byte g, byte b)
    {
        const int side = 8;
        const int headerSize = 54; // BITMAPFILEHEADER (14) + BITMAPINFOHEADER (40)
        var rowSize = (side * 3 + 3) / 4 * 4; // 24-bit rows padded to 4 bytes
        var bmp = new byte[headerSize + rowSize * side];

        // BITMAPFILEHEADER: "BM", file size, and the pixel-data offset.
        bmp[0] = (byte)'B';
        bmp[1] = (byte)'M';
        WriteUInt32(bmp, 2, (uint)bmp.Length);
        WriteUInt32(bmp, 10, headerSize);

        // BITMAPINFOHEADER: 40-byte header, dimensions, 1 plane, 24 bpp, BI_RGB.
        // The remaining fields stay zero (the array is zero-initialized).
        WriteUInt32(bmp, 14, 40);
        WriteInt32(bmp, 18, side);
        WriteInt32(bmp, 22, side);
        WriteUInt16(bmp, 26, 1);
        WriteUInt16(bmp, 28, 24);

        // Solid BGR pixels (bottom-up rows, padding left zero).
        for (var y = 0; y < side; y++)
        {
            var rowStart = headerSize + y * rowSize;
            for (var x = 0; x < side; x++)
            {
                var pixel = rowStart + x * 3;
                bmp[pixel] = b;
                bmp[pixel + 1] = g;
                bmp[pixel + 2] = r;
            }
        }

        using var stream = new MemoryStream(bmp);
        var bitmap = new BitmapImage();
        bitmap.SetSource(stream.AsRandomAccessStream());
        return bitmap;
    }

    private static void WriteUInt32(byte[] buffer, int offset, uint value)
    {
        buffer[offset] = (byte)value;
        buffer[offset + 1] = (byte)(value >> 8);
        buffer[offset + 2] = (byte)(value >> 16);
        buffer[offset + 3] = (byte)(value >> 24);
    }

    private static void WriteInt32(byte[] buffer, int offset, int value) =>
        WriteUInt32(buffer, offset, (uint)value);

    private static void WriteUInt16(byte[] buffer, int offset, ushort value)
    {
        buffer[offset] = (byte)value;
        buffer[offset + 1] = (byte)(value >> 8);
    }

    // Placeholder header values for the album-expansion header block.
    internal static string ExpansionTitle { get; } = "Album Title";

    internal static string ExpansionArtist { get; } = "Artist Name";

    // Placeholder track rows for the album-expansion track row: position, title,
    // an optional row artist (core names one only for a compilation), and the
    // duration. The array type is stated so the null artist is typed.
    internal static IReadOnlyList<(string Position, string Title, string? Artist, string Duration)> ExpansionTracks { get; } =
        new (string Position, string Title, string? Artist, string Duration)[]
        {
            ("1", "Track Title One", null, "3:14"),
            ("2", "Track Title Two", "Guest Artist", "4:07"),
            ("3", "Track Title Three", null, "2:58"),
        };
}
#endif
