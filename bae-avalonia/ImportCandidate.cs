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

    public ImportCandidateRowStatus RowStatus { get; set; } = new();

    /// <summary>Candidate identities from a "found" result; empty otherwise.</summary>
    public List<ReleaseCandidateChoice> Matches { get; set; } = new();

    /// <summary>Every file in the folder exactly once, with the role in force
    /// for it, what that role makes of it, the roles it can be put in, and —
    /// for a track sheet — what it describes. The mapping table itself is core's
    /// projection over the same folder; this is what the pane reads for the
    /// facts that sit outside it, the source-audio summary and its images. Null
    /// before a candidate has been read.
    /// </summary>
    internal BridgeCandidateFiles? Files { get; set; }

    /// <summary>The folder's image files as cover choices.</summary>
    internal List<LocalArtwork> LocalArtwork { get; set; } = new();

    /// <summary>Everything the pane draws, as core reads it back for this key:
    /// the picked release, the metadata form, the mapping table, the cover, the
    /// evidence badge, the last failed import. Null until the per-candidate read
    /// has answered. The pane keeps no copy of any of it — a control writes,
    /// core commits, and the next value of this lands here.</summary>
    internal BridgeImportCandidateDetail? Detail { get; set; }

    /// <summary>The picked release as its archived documents describe it.</summary>
    internal BridgeReleaseDetail? Release => Detail?.Release;

    /// <summary>What identified the picked release, each entry naming the file
    /// it was read off — the chip that file's gallery tile or table row
    /// carries.</summary>
    internal IReadOnlyList<BridgeFileEvidence> FileEvidence =>
        Detail?.FileEvidence ?? System.Array.Empty<BridgeFileEvidence>();

    /// <summary>The candidate's one editable metadata draft.</summary>
    internal BridgeRawReleaseEdit? Edit => Detail?.MetadataDraft;

    /// <summary>Every source unit the folder offers with the track committing
    /// makes of it. An empty table until the first read answers.</summary>
    internal BridgeMappingTable Mapping =>
        Detail?.Mapping ?? new BridgeMappingTable([], [], [], Reconciliation: null);

    /// <summary>The cover this candidate commits with.</summary>
    internal BridgeCoverChoice? Cover => Detail?.Cover;

    /// <summary>The last import of this candidate that failed.</summary>
    internal BridgeImportFailure? Failure => Detail?.Failure;

    /// <summary>Where the current draft began. Direct entry has none.</summary>
    internal BridgeMetadataProvenance? MetadataProvenance =>
        Detail?.MetadataProvenance;

    /// <summary>The release this candidate is selected as, where provenance names
    /// one.</summary>
    internal BridgeMetadataProvenance.ExternalRelease? PickedRelease =>
        MetadataProvenance as BridgeMetadataProvenance.ExternalRelease;

    /// <summary>The draft or temporary source browser occupying the metadata
    /// slot. Browsing never replaces the stored draft.</summary>
    internal ImportMetadataPresentation MetadataPresentation { get; private set; } =
        ImportMetadataPresentation.Draft;

    /// <summary>The state of this candidate's lazy File Tags preview.</summary>
    internal ImportFileTagsPreviewStatus FileTagsPreviewStatus { get; private set; } =
        ImportFileTagsPreviewStatus.Unloaded;

    internal BridgeReleaseUserEdit? FileTagsPreview { get; private set; }
    internal string? FileTagsPreviewError { get; private set; }
    private object? _fileTagsPreviewSession;

    internal void ResolveInitialMetadataPresentation() =>
        MetadataPresentation = MetadataProvenance is not null
            ? ImportMetadataPresentation.Draft
            : Detail?.InitialMetadataSource switch
            {
                BridgeDefaultImportMetadataSource.FindOnline =>
                    ImportMetadataPresentation.FindOnline,
                BridgeDefaultImportMetadataSource.FileTags =>
                    ImportMetadataPresentation.FileTags,
                BridgeDefaultImportMetadataSource.None =>
                    ImportMetadataPresentation.Draft,
                null => ImportMetadataPresentation.Draft,
                _ => throw new System.ArgumentOutOfRangeException(
                    nameof(Detail.InitialMetadataSource),
                    Detail.InitialMetadataSource,
                    "Unknown default metadata source"),
            };

    internal void PresentMetadata(ImportMetadataPresentation presentation) =>
        MetadataPresentation = presentation;

    /// <summary>Carry navigation and the lazy preview over a fresh live detail
    /// for this same candidate. A changed file set invalidates the preview.</summary>
    internal void PreserveSessionState(ImportCandidate existing)
    {
        MetadataPresentation = existing.MetadataPresentation;
        if (existing.Files?.FileTagsIdentity != Files?.FileTagsIdentity)
        {
            return;
        }
        FileTagsPreviewStatus = existing.FileTagsPreviewStatus;
        FileTagsPreview = existing.FileTagsPreview;
        FileTagsPreviewError = existing.FileTagsPreviewError;
        _fileTagsPreviewSession = existing._fileTagsPreviewSession;
    }

    internal object? BeginFileTagsPreview()
    {
        if (MetadataProvenance is BridgeMetadataProvenance.FileTags
            || FileTagsPreviewStatus is ImportFileTagsPreviewStatus.Loading
                or ImportFileTagsPreviewStatus.Loaded)
        {
            return null;
        }
        var session = new object();
        _fileTagsPreviewSession = session;
        FileTagsPreviewStatus = ImportFileTagsPreviewStatus.Loading;
        FileTagsPreviewError = null;
        return session;
    }

    internal bool CompleteFileTagsPreview(object session, BridgeReleaseUserEdit edit)
    {
        if (!ReferenceEquals(_fileTagsPreviewSession, session))
        {
            return false;
        }
        _fileTagsPreviewSession = null;
        FileTagsPreviewStatus = ImportFileTagsPreviewStatus.Loaded;
        FileTagsPreview = edit;
        FileTagsPreviewError = null;
        return true;
    }

    internal bool FailFileTagsPreview(object session, string? error)
    {
        if (!ReferenceEquals(_fileTagsPreviewSession, session))
        {
            return false;
        }
        _fileTagsPreviewSession = null;
        FileTagsPreviewStatus = ImportFileTagsPreviewStatus.Failed;
        FileTagsPreview = null;
        FileTagsPreviewError = error;
        return true;
    }

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

        return string.Join("  ·  ", parts);
    }
}

internal enum ImportMetadataPresentation
{
    Draft,
    FindOnline,
    FileTags,
}

internal enum ImportFileTagsPreviewStatus
{
    Unloaded,
    Loading,
    Loaded,
    Failed,
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
    public int? ProgressPercent { get; set; }
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
            if (ProgressPercent is null)
            {
                return string.IsNullOrEmpty(stepLabel)
                    ? Loc.Chrome("import.progress.identifying")
                    : stepLabel;
            }
            var importing = Loc.Chrome("import.progress.percent", "percent", ProgressPercent.Value);
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
