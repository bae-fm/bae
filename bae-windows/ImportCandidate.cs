using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// A release candidate found by a folder scan. <see cref="Key"/> is the
/// candidate's folder path — the identity used for the later identify/commit
/// steps.
/// </summary>
public sealed class ImportCandidate
{
    public string Key { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public int TrackCount { get; set; }
    public string Format { get; set; } = string.Empty;

    public ImportCandidateRowStatus RowStatus { get; set; } = new();
    public string StatusOverride { get; set; } = string.Empty;

    /// <summary>The auto-identification or import status shown in the row.</summary>
    public string Status
    {
        get
        {
            if (!string.IsNullOrEmpty(StatusOverride))
            {
                return StatusOverride;
            }
            return RowStatus.LocalizedLine;
        }
    }

    /// <summary>Candidate identities from a "found" result; empty otherwise.</summary>
    public List<ReleaseCandidateChoice> Matches { get; set; } = new();

    /// <summary>The signals-toolbar badges (disc ID, barcode, catalog), each with
    /// its live lookup state. Core pre-shapes the list; the row iterates and
    /// renders it.</summary>
    public List<SignalBadge> Signals { get; set; } = new();

    /// <summary>The candidate's playable audio files, for pre-import preview.</summary>
    public List<string> AudioPaths { get; set; } = new();

    /// <summary>The on-disk folder to identify/import.</summary>
    public string FolderPath { get; set; } = string.Empty;

    /// <summary>The list row, omitting absent fields. Used as the default item text.</summary>
    public override string ToString()
    {
        var parts = new List<string> { Name };
        parts.Add(Loc.Chrome("import.candidate.tracks", "count", TrackCount));

        if (!string.IsNullOrEmpty(Format))
        {
            parts.Add(Format);
        }

        var line = string.Join("  ·  ", parts);
        return string.IsNullOrEmpty(Status) ? line : $"{line}  —  {Status}";
    }
}

public sealed class ImportCandidateRowStatus
{
    public string Kind { get; set; } = string.Empty;
    public int Count { get; set; }
    public int ProgressPercent { get; set; }
    public ImportStep? Step { get; set; }
    internal BridgeException? Error { get; set; }
    internal BridgeInvalidReason? InvalidReason { get; set; }

    public string LocalizedLine
    {
        get
        {
            return Kind switch
            {
                "identifying" => Loc.Chrome("identify.identifying"),
                "found" => Loc.Chrome("identify.found", "count", Count),
                "conflict" => Loc.Chrome("identify.conflict"),
                "not_found" => Loc.Chrome("identify.not_found"),
                "manual" => Loc.Chrome("identify.manual"),
                "importing" => ImportingLine,
                "complete" => Loc.Chrome("import.complete"),
                "error" => ErrorLine is null
                    ? Loc.Chrome("import.failed")
                    : $"{Loc.Chrome("import.failed")}: {ErrorLine}",
                _ => string.Empty,
            };
        }
    }

    private string? ErrorLine => InvalidReason is { } reason
        ? BridgeDisplay.LocalizedLine(reason)
        : Error is { } error
            ? BridgeDisplay.LocalizedLine(error)
            : null;

    private string ImportingLine
    {
        get
        {
            var stepLabel = Step?.LocalizedLabel;
            var importing = Loc.Chrome("import.progress.percent", "percent", ProgressPercent);
            return string.IsNullOrEmpty(stepLabel) ? importing : $"{importing} — {stepLabel}";
        }
    }
}

public sealed class ImportStep
{
    public string Kind { get; set; } = string.Empty;
    public string? StepTag { get; set; }
    public string? Phase { get; set; }

    public string LocalizedLabel
    {
        get
        {
            var key = Kind switch
            {
                "preparing" when StepTag is not null => NativeBae.PrepareStepKey(StepTag),
                "running" when Phase is not null => NativeBae.ImportPhaseKey(Phase),
                _ => null,
            };
            return key is null ? string.Empty : Loc.Core(key);
        }
    }
}
