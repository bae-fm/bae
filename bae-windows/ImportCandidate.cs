using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// A release candidate found by a folder scan (a <c>CandidateAdded</c> event).
/// <see cref="Key"/> is the candidate's folder path — the identity used for the
/// later identify/commit steps.
/// </summary>
public sealed class ImportCandidate
{
    public string Key { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public int? TrackCount { get; set; }
    public string Format { get; set; } = string.Empty;

    /// <summary>The auto-identification status, e.g. "identifying…" / "found (3)".</summary>
    public string Status { get; set; } = string.Empty;

    /// <summary>Candidate identities from a "found" result; empty otherwise.</summary>
    public List<Candidate> Matches { get; set; } = new();

    /// <summary>The signals-toolbar badges (disc ID, barcode, catalog), each with
    /// its live lookup state. Core pre-shapes the list; the row iterates and renders
    /// it. Empty until the first <c>CandidateIdentifyState</c> event arrives.</summary>
    public List<SignalBadge> Signals { get; set; } = new();

    /// <summary>The candidate's playable audio files, for pre-import preview.</summary>
    public List<string> AudioPaths { get; set; } = new();

    /// <summary>The on-disk folder to identify/import.</summary>
    public string FolderPath { get; set; } = string.Empty;

    /// <summary>The list row, omitting absent fields. Used as the default item text.</summary>
    public override string ToString()
    {
        var parts = new List<string> { Name };
        if (TrackCount is int count)
        {
            parts.Add($"{count} tracks");
        }

        if (!string.IsNullOrEmpty(Format))
        {
            parts.Add(Format);
        }

        var line = string.Join("  ·  ", parts);
        return string.IsNullOrEmpty(Status) ? line : $"{line}  —  {Status}";
    }
}
