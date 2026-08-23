using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// One signals-toolbar badge. Core derives all per-signal state; the UI
/// iterates and renders these directly. <see cref="Kind"/>
/// is the snake_case wire name the badge view maps to an icon / label.
/// </summary>
public sealed class SignalBadge
{
    /// <summary>"disc_id" / "barcode" / "catalog".</summary>
    public string Kind { get; set; } = string.Empty;

    /// <summary>The badge value (disc-ID hash, barcode digits, catalog number),
    /// or null when an identity signal had no value to show.</summary>
    public string? Value { get; set; }

    /// <summary>The live lookup/match state — the badge's trailing visual.</summary>
    public SignalBadgeState State { get; set; } = new();

    /// <summary>Whether the user excluded this signal from triangulation. Excluded
    /// badges still render (dimmed, struck through) so the row stays stable.</summary>
    public bool Excluded { get; set; }

    /// <summary>The values this signal could take, for the signals that offer a
    /// choice. Empty for the disc ID and the barcode, which have one value
    /// each; the catalog's are every number extracted from the candidate.</summary>
    public IReadOnlyList<SignalBadgeOption> Options { get; set; } = [];
}

/// <summary>One of the values a signal could take. At most one option of a
/// signal is chosen — the one the identify run looks up.</summary>
public sealed class SignalBadgeOption
{
    public string Value { get; set; } = string.Empty;
    public bool Chosen { get; set; }
}

/// <summary>
/// A badge's live lookup state. <see cref="Count"/> is set for "found",
/// <see cref="Failure"/> for "failed", both null otherwise. The locale never
/// crosses the bridge, so the failed state carries the generated bridge
/// failure, not a prose message.
/// </summary>
public sealed class SignalBadgeState
{
    /// <summary>"looking_up" / "found" / "no_match" / "skipped" / "failed".</summary>
    public string Kind { get; set; } = string.Empty;
    public uint? Count { get; set; }

    /// <summary>The structured lookup failure for the "failed" state; null
    /// otherwise. The badge resolves its localized line from this.</summary>
    internal BridgeLookupFailure? Failure { get; set; }
}
