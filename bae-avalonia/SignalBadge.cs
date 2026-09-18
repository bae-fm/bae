using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// One signals-toolbar badge. Core derives all per-signal state; the UI
/// iterates and renders these directly.
/// </summary>
public sealed class SignalBadge
{
    /// <summary>Which signal this badge stands for. Internal: the generated
    /// bridge enums are internal, so a public member exposing one is
    /// inconsistent accessibility (CS0053).</summary>
    internal BridgeSignalKind Kind { get; set; } = BridgeSignalKind.DiscId;

    /// <summary>The badge value (disc-ID hash, barcode digits, catalog number),
    /// or null when an identity signal had no value to show.</summary>
    public string? Value { get; set; }

    /// <summary>The live lookup/match state — the badge's trailing visual.
    /// Internal for the same reason as <see cref="Kind"/>.</summary>
    internal BridgeSignalState State { get; set; } = new BridgeSignalState.Skipped();

    /// <summary>Whether the run asks about none of this signal's values.
    /// Excluded badges still render (dimmed, struck through) so the row stays
    /// stable.</summary>
    public bool Excluded { get; set; }

    /// <summary>The values this signal offers, each marked when the run asks
    /// about it. Empty for the disc ID, which has one value the badge itself
    /// stands for, and for a signal the candidate carries no value of.</summary>
    public IReadOnlyList<SignalBadgeOption> Options { get; set; } = [];
}

/// <summary>One of the values a signal offers. Several options of a signal can
/// be chosen at once — every one the identify run asks about.</summary>
public sealed class SignalBadgeOption
{
    public string Value { get; set; } = string.Empty;
    public bool Chosen { get; set; }
}
