#if DEBUG
using System.Collections.Generic;

namespace Bae.Windows;

// Static fixtures for the debug component gallery — the Windows analogue of the
// macOS PreviewData set. Generic placeholder values only: no real
// artist/album/song names, no bridge, keychain, or library access. Compiled
// only in DEBUG builds, alongside the gallery it feeds.
internal static class PreviewData
{
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
}
#endif
