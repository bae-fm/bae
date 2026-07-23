#if DEBUG
using System.Collections.Generic;
using Microsoft.UI.Xaml.Media;
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
    // generic placeholder names, no cover art. The cover is null on purpose — a
    // BitmapImage source decodes asynchronously, and RenderTargetBitmap blocks the
    // UI thread waiting on a decode that never completes on a headless runner, so
    // the tile shows the card's flat placeholder background (its real state before
    // a cover loads) instead.
    internal static IReadOnlyList<(string Title, string Artist, string Year, ImageSource? Cover)> GridCards { get; } =
        new (string Title, string Artist, string Year, ImageSource? Cover)[]
        {
            ("Album Title One", "Artist One", "2021", null),
            ("Album Title Two", "Artist Two", "2019", null),
            ("Album Title Three", "Artist Three", "2020", null),
            ("Album Title Four", "Artist Four", "2017", null),
            ("Album Title Five", "Artist Five", "2022", null),
            ("Album Title Six", "Artist Six", "2016", null),
            ("Album Title Seven", "Artist Seven", "2018", null),
            ("Album Title Eight", "Artist Eight", "2023", null),
            ("Album Title Nine", "Artist Nine", "2015", null),
            ("Album Title Ten", "Artist Ten", "2021", null),
            ("Album Title Eleven", "Artist Eleven", "2019", null),
            ("Album Title Twelve", "Artist Twelve", "2020", null),
        };

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
