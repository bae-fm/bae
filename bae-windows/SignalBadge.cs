namespace Bae.Windows;

/// <summary>
/// One signals-toolbar badge (from a <c>CandidateIdentifyState</c> event) — a
/// flat, pre-shaped mirror of the FFI's <c>FfiSignal</c>. Core derives all
/// per-signal state; the UI iterates and renders these directly. <see cref="Kind"/>
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
}

/// <summary>
/// A badge's live lookup/match state — a tag plus the one payload each variant
/// carries. Mirrors the FFI's <c>FfiSignalState</c>: <see cref="Count"/> is set
/// for "found"/"confirms", <see cref="Failure"/> for "failed", both null
/// otherwise. The locale never crosses the bridge, so the failed state carries
/// the structured <see cref="LookupFailure"/>, not a prose message.
/// </summary>
public sealed class SignalBadgeState
{
    /// <summary>"looking_up" / "found" / "no_match" / "skipped" / "failed" / "confirms".</summary>
    public string Kind { get; set; } = string.Empty;
    public uint? Count { get; set; }

    /// <summary>The structured lookup failure for the "failed" state; null
    /// otherwise. The badge resolves its localized line from this.</summary>
    public LookupFailure? Failure { get; set; }
}
