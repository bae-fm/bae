using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

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

    /// <summary>Every file in the folder exactly once, with the role in force
    /// for it, what that role makes of it, the roles it can be put in, and —
    /// for a track sheet — what it describes. The mapping table itself is core's
    /// projection over the same folder; this is what the pane reads for the
    /// facts that sit outside it, the folder's format and its images. Null
    /// before a candidate has been read.
    /// </summary>
    internal BridgeCandidateFiles? Files { get; set; }

    /// <summary>The folder's image files as cover choices.</summary>
    internal List<LocalArtwork> LocalArtwork { get; set; } = new();

    /// <summary>The on-disk folder to identify/import.</summary>
    public string FolderPath { get; set; } = string.Empty;

    /// <summary>The user manually skipped this candidate.</summary>
    public bool Skipped { get; set; }

    /// <summary>Already imported (content-hash match).</summary>
    public bool IsAdded { get; set; }

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

/// <summary>A candidate's readable evidence file (CUE sheet, rip log, info
/// text): its display name and absolute path.</summary>
public sealed class ImportDocument
{
    public string Name { get; set; } = string.Empty;
    public string Path { get; set; } = string.Empty;
}

/// <summary>One of a candidate's audio files, as a choice for a track sheet's
/// binding. Core decides which are usable, so no UI reads a codec to work out
/// what it may offer.</summary>
public sealed class ImportSheetBindingOption
{
    /// <summary>The audio file's release-relative path.</summary>
    public string FileId { get; set; } = string.Empty;

    /// <summary>Why the sheet cannot use it, in the user's language. Null when
    /// it can.</summary>
    public string? RefusalReason { get; set; }
}

public sealed class ImportCandidateRowStatus
{
    public string Kind { get; set; } = string.Empty;
    public int Count { get; set; }
    public int ProgressPercent { get; set; }
    public ImportStep? Step { get; set; }
    internal BridgeException? Error { get; set; }

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

    private string? ErrorLine => Error is { } error ? BridgeDisplay.LocalizedLine(error) : null;

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
                "preparing" when StepTag is not null => BridgeDisplay.PrepareStepKey(StepTag),
                "running" when Phase is not null => BridgeDisplay.ImportPhaseKey(Phase),
                _ => null,
            };
            return key is null ? string.Empty : Loc.Core(key);
        }
    }
}
