using System.Linq;
using System.Text.Json;
using uniffi.bae_bridge;


namespace Bae.Desktop;

internal static partial class NativeBae
{
    // AppHandle operations reach this adapter only from the session's worker.
    // Block that worker until UniFFI completes so LibraryHandle keeps the native
    // handle borrowed for the operation's full lifetime. The caller owns the
    // only UI-to-worker dispatch; adding another Task.Run here would duplicate
    // that boundary for every generated async method.
    private static T Await<T>(Func<System.Threading.Tasks.Task<T>> call) =>
        call().GetAwaiter().GetResult();

    private static void Await(Func<System.Threading.Tasks.Task> call) =>
        call().GetAwaiter().GetResult();

    private static string? CaptureError(Action action)
    {
        try
        {
            action();
            return null;
        }
        catch (BridgeException.Cancelled)
        {
            return null;
        }
        catch (BridgeException exception)
        {
            return exception.Message;
        }
    }

    private static async System.Threading.Tasks.Task<string?> CaptureError(
        Func<System.Threading.Tasks.Task> action)
    {
        try
        {
            await action();
            return null;
        }
        catch (BridgeException.Cancelled)
        {
            return null;
        }
        catch (BridgeException exception)
        {
            return exception.Message;
        }
    }

    private static string? CaptureValue(Func<string?> action)
    {
        try
        {
            return action();
        }
        catch (BridgeException.Cancelled)
        {
            return null;
        }
        catch (BridgeException exception)
        {
            return exception.Message;
        }
    }

    private static (T? Value, string? Error) CaptureBridgeValue<T>(Func<T> action) where T : class
    {
        try
        {
            return (action(), null);
        }
        catch (BridgeException.Cancelled)
        {
            return (null, null);
        }
        catch (BridgeException exception)
        {
            return (null, exception.Message);
        }
    }

    private static T? Capture<T>(Func<T?> action)
        where T : class
    {
        try
        {
            return action();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Warning($"image bytes read failed: {exception.Message}");
            return null;
        }
    }

    private static BridgeSortCriterion[] ToBridge(IReadOnlyList<SortCriterion<AlbumSortField>> criteria) =>
        criteria.Select(criterion =>
            new BridgeSortCriterion(ToBridge(criterion.Field), ToBridge(criterion.Direction))).ToArray();

    private static BridgeComposerSortCriterion[] ToBridge(IReadOnlyList<SortCriterion<ComposerSortField>> criteria) =>
        criteria.Select(criterion =>
            new BridgeComposerSortCriterion(ToBridge(criterion.Field), ToBridge(criterion.Direction))).ToArray();

    private static BridgeArtistSortCriterion[] ToBridge(IReadOnlyList<SortCriterion<ArtistSortField>> criteria) =>
        criteria.Select(criterion =>
            new BridgeArtistSortCriterion(ToBridge(criterion.Field), ToBridge(criterion.Direction))).ToArray();

    private static BridgeSortField ToBridge(AlbumSortField field) => field switch
    {
        AlbumSortField.DateAdded => BridgeSortField.DateAdded,
        AlbumSortField.Title => BridgeSortField.Title,
        AlbumSortField.Artist => BridgeSortField.Artist,
        AlbumSortField.Year => BridgeSortField.Year,
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown album sort field"),
    };

    private static BridgeComposerSortField ToBridge(ComposerSortField field) => field switch
    {
        ComposerSortField.Name => BridgeComposerSortField.Name,
        ComposerSortField.WorkCount => BridgeComposerSortField.WorkCount,
        ComposerSortField.LinkedReleaseCount => BridgeComposerSortField.LinkedReleaseCount,
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown composer sort field"),
    };

    private static BridgeArtistSortField ToBridge(ArtistSortField field) => field switch
    {
        ArtistSortField.Name => BridgeArtistSortField.Name,
        ArtistSortField.AlbumCount => BridgeArtistSortField.AlbumCount,
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown artist sort field"),
    };

    private static BridgeSortDirection ToBridge(SortDirection direction) => direction switch
    {
        SortDirection.Ascending => BridgeSortDirection.Ascending,
        SortDirection.Descending => BridgeSortDirection.Descending,
        _ => throw new ArgumentOutOfRangeException(nameof(direction), direction, "Unknown sort direction"),
    };

    private static Settings Settings(
        BridgeConfig config,
        BridgeMcpServerStatus mcpStatus,
        BridgeSubsonicServerStatus subsonicStatus) =>
        new()
        {
            LibraryName = config.LibraryName,
            LibraryId = config.LibraryId,
            DiscogsStatus = DiscogsStatusTag(config.DiscogsTokenStatus),
            DiscogsUsable = config.DiscogsUsable,
            SyncProvider = config.Sync is null ? null : SyncProviderTag(config.Sync.Provider),
            SyncAccount = config.Sync?.CloudAccountDisplay,
            PauseBetweenSides = config.PauseBetweenSides,
            IdentifyAutomatically = config.IdentifyAutomatically,
            DefaultImportMetadataSource = config.DefaultImportMetadataSource,
            ShowRemainingTime = config.ShowRemainingTime,
            LibraryFullWidth = config.LibraryFullWidth,
            SavePresets = config.SavePresets.Select(SavePreset).ToList(),
            DefaultTrackSavePreset = config.DefaultTrackSavePreset,
            DefaultReleaseSavePreset = config.DefaultReleaseSavePreset,
            CastEnabled = config.CastEnabled,
            McpEnabled = config.Mcp.Enabled,
            McpPort = config.Mcp.Port,
            McpStatus = mcpStatus,
            SubsonicEnabled = config.Subsonic.Enabled,
            SubsonicPort = config.Subsonic.Port,
            SubsonicUsername = config.Subsonic.Username,
            SubsonicBindAddress = config.Subsonic.BindAddress,
            SubsonicStatus = subsonicStatus,
        };


    private static List<LocalArtwork> LocalArtwork(BridgeCandidateFiles files) =>
        files.Files.Select(LocalArtwork).OfType<LocalArtwork>().ToList();

    // The candidate's images, as cover choices. A file that is not an image has
    // no choice to offer, so it drops out.
    private static LocalArtwork? LocalArtwork(BridgeCandidateFile file)
    {
        var choice = file.Role switch
        {
            BridgeFileRole.Artwork artwork => artwork.Choice,
            _ => null,
        };
        if (choice is null)
        {
            return null;
        }
        var releaseImage = choice.Selection as BridgeCoverSelection.ReleaseImage
            ?? throw new JsonException("local artwork did not carry a release-image selection");
        return new LocalArtwork
        {
            FileId = releaseImage.FileId,
            Path = file.File.LocalPath,
        };
    }

    /// <summary>One candidate as its tables describe it: the folder, where its
    /// import stands, and the identity its stored verdict stands back up as.
    /// What is running for it right now is not here — a control that draws that
    /// reads it off the candidate-runtime signal for its own key.</summary>
    internal static ImportCandidate ImportCandidateRow(
        BridgeImportCandidateDetail detail)
    {
        var candidate = detail.Candidate;
        var row = new ImportCandidate
        {
            Key = candidate.FolderPath,
            Name = candidate.SourceFolderName,
            TrackCount = checked((int)candidate.TrackCount),
            RowStatus = StoredRowStatus(detail),
            Matches = ImportMatches(detail.ResumedIdentifyState),
            Files = candidate.Files,
            LocalArtwork = LocalArtwork(candidate.Files),
            FolderPath = candidate.FolderPath,
            Skipped = candidate.Skipped,
            IsAdded = candidate.IsAdded,
            Detail = detail,
        };
        return row;
    }

    /// <summary>What the tables say about a candidate: where its import stands
    /// first — a running import outranks any answer — then the identity its
    /// stored verdict resumes.</summary>
    private static ImportCandidateRowStatus StoredRowStatus(
        BridgeImportCandidateDetail detail) => detail.Row.ImportStatus switch
        {
            BridgeTriageImportStatus.Importing => new ImportCandidateRowStatus
            {
                Kind = "importing",
            },
            BridgeTriageImportStatus.Complete => new ImportCandidateRowStatus { Kind = "complete" },
            BridgeTriageImportStatus.Error error => new ImportCandidateRowStatus
            {
                Kind = "error",
                Error = error.ErrorValue,
            },
            _ => IdentifyRowStatus(detail.ResumedIdentifyState),
        };

    /// <summary>What is happening for one key right now: its status, the
    /// pressings the run is offering, and the badge row it carries. A control
    /// watching one key renders this over what the candidate's tables say.
    /// </summary>
    internal static (
        ImportCandidateRowStatus? RowStatus,
        List<ReleaseCandidateChoice> Matches,
        List<SignalBadge> Signals) ImportRun(
        BridgeCandidateRuntimeSnapshot? runtime) =>
        (RuntimeRowStatus(runtime), RuntimeMatches(runtime), RuntimeSignals(runtime));

    /// <summary>The running import with how far it has got, else the live
    /// run's state. Null when neither — what the candidate's tables say stands.
    /// </summary>
    private static ImportCandidateRowStatus? RuntimeRowStatus(
        BridgeCandidateRuntimeSnapshot? runtime)
    {
        if (runtime is null)
        {
            return null;
        }
        if (runtime.Import is { } running)
        {
            return new ImportCandidateRowStatus
            {
                Kind = "importing",
                ProgressPercent = running.ProgressPercent is { } percent
                    ? checked((int)percent)
                    : null,
                Step = running.Step is null ? null : ImportStep(running.Step),
            };
        }
        return runtime.IdentifyState is BridgeIdentifyState.Idle
            ? null
            : IdentifyRowStatus(runtime.IdentifyState);
    }

    /// <summary>The matches a run in flight is offering, or an empty list.
    /// </summary>
    private static List<ReleaseCandidateChoice> RuntimeMatches(
        BridgeCandidateRuntimeSnapshot? runtime) =>
        runtime is null ? [] : ImportMatches(runtime.IdentifyState);

    /// <summary>The badge row a run in flight carries, or an empty list.
    /// </summary>
    private static List<SignalBadge> RuntimeSignals(
        BridgeCandidateRuntimeSnapshot? runtime) =>
        runtime is null
            ? []
            : runtime.SignalsToolbar.Signals.Select(SignalBadge).ToList();

    private static ImportCandidateRowStatus IdentifyRowStatus(BridgeIdentifyState state) =>
        state switch
        {
            BridgeIdentifyState.Idle => new ImportCandidateRowStatus { Kind = string.Empty },
            BridgeIdentifyState.Triangulating => new ImportCandidateRowStatus { Kind = "identifying" },
            BridgeIdentifyState.Found found => new ImportCandidateRowStatus { Kind = "found", Count = found.Groups.Sum(group => group.Pressings.Length) },
            BridgeIdentifyState.NotFoundAnywhere => new ImportCandidateRowStatus { Kind = "not_found" },
            BridgeIdentifyState.ManualOnly => new ImportCandidateRowStatus { Kind = "manual" },
            BridgeIdentifyState.Failed => new ImportCandidateRowStatus { Kind = "failed" },
            _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown identify state"),
        };

    /// <summary>The pressings a state is offering. A failed state offers what
    /// the provider that did answer found, so it lists its groups too.
    /// </summary>
    private static List<ReleaseCandidateChoice> ImportMatches(BridgeIdentifyState state) =>
        state switch
        {
            BridgeIdentifyState.Found found => GroupChoices(found.Groups),
            BridgeIdentifyState.Failed failed => GroupChoices(failed.Groups),
            _ => [],
        };

    /// <summary>One choice per pressing row, picking the row's first source's
    /// release — core orders the row's releases, MusicBrainz first.</summary>
    internal static List<ReleaseCandidateChoice> GroupChoices(
        IEnumerable<BridgeReleaseGroup> groups) =>
        groups
            .SelectMany(group => group.Pressings
                .Select(pressing => new ReleaseCandidateChoice(group, pressing.Releases[0])))
            .ToList();

    private static SignalBadge SignalBadge(BridgeToolbarSignal signal) =>
        new()
        {
            Kind = SignalKindTag(signal.Kind),
            Value = signal.Value,
            State = SignalState(signal.State),
            Excluded = signal.Excluded,
            Options = signal.Options
                .Select(option => new SignalBadgeOption
                {
                    Value = option.Value,
                    Chosen = option.Chosen,
                })
                .ToList(),
        };

    private static SignalBadgeState SignalState(BridgeSignalState state) =>
        state switch
        {
            BridgeSignalState.LookingUp => new() { Kind = "looking_up" },
            BridgeSignalState.Found found => new() { Kind = "found", Count = found.Count },
            BridgeSignalState.NoMatch => new() { Kind = "no_match" },
            BridgeSignalState.Skipped => new() { Kind = "skipped" },
            BridgeSignalState.Failed failed => new() { Kind = "failed", Failure = failed.Failure },
            _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown signal state"),
        };

    private static ImportStep ImportStep(BridgeImportStep step) =>
        step switch
        {
            BridgeImportStep.Preparing preparing => new() { Kind = "preparing", StepTag = PrepareStepTag(preparing.Step) },
            BridgeImportStep.Running running => new() { Kind = "running", Phase = ImportPhaseTag(running.Phase) },
            _ => throw new ArgumentOutOfRangeException(nameof(step), step, "Unknown import step"),
        };

    private static string CoverImageSourceUrl(BridgeCoverImageSource source) =>
        source switch
        {
            BridgeCoverImageSource.Remote remote => remote.Url,
            BridgeCoverImageSource.Local local => local.Path,
            _ => throw new ArgumentOutOfRangeException(nameof(source), source, "Unknown cover image source"),
        };

    private static SavePreset SavePreset(BridgeSavePreset preset) =>
        new()
        {
            Id = preset.Id,
            Name = preset.Name,
            Codec = preset.Codec,
            Extension = preset.Extension,
            FilenameTokens = preset.FilenameTokens.ToList(),
            PregapPlacement = preset.PregapPlacement,
            AppliesToTrack = preset.AppliesToTrack,
            AppliesToRelease = preset.AppliesToRelease,
            EmbedCover = preset.EmbedCover,
        };

    private static BridgeSavePreset SavePresetBridge(SavePreset preset) =>
        new(
            preset.Id,
            preset.Name,
            preset.Codec,
            preset.Extension,
            preset.FilenameTokens.ToArray(),
            preset.PregapPlacement,
            preset.AppliesToTrack,
            preset.AppliesToRelease,
            preset.EmbedCover);

    private static string DiscogsStatusTag(BridgeDiscogsTokenStatus status) =>
        status switch
        {
            BridgeDiscogsTokenStatus.NotConfigured => "not_configured",
            BridgeDiscogsTokenStatus.Valid => "valid",
            BridgeDiscogsTokenStatus.Unvalidated => "unvalidated",
            BridgeDiscogsTokenStatus.Rejected => "rejected",
            _ => throw new ArgumentOutOfRangeException(nameof(status), status, "Unknown Discogs token status"),
        };

    private static string SyncProviderTag(BridgeSyncProvider provider) =>
        provider switch
        {
            BridgeSyncProvider.S3 => "s3",
            BridgeSyncProvider.GoogleDrive => "google_drive",
            BridgeSyncProvider.Dropbox => "dropbox",
            BridgeSyncProvider.OneDrive => "onedrive",
            BridgeSyncProvider.CloudKit => "cloudkit",
            _ => throw new ArgumentOutOfRangeException(nameof(provider), provider, "Unknown sync provider"),
        };

    private static string SignalKindTag(BridgeSignalKind kind) =>
        kind switch
        {
            BridgeSignalKind.DiscId => "disc_id",
            BridgeSignalKind.Barcode => "barcode",
            BridgeSignalKind.Catalog => "catalog",
            _ => throw new ArgumentOutOfRangeException(nameof(kind), kind, "Unknown signal kind"),
        };

    private static string PrepareStepTag(BridgePrepareStep step) =>
        step switch
        {
            BridgePrepareStep.Queued => "queued",
            BridgePrepareStep.ValidatingSourceFiles => "validating_source_files",
            _ => throw new ArgumentOutOfRangeException(nameof(step), step, "Unknown prepare step"),
        };

    private static string ImportPhaseTag(BridgeImportPhase phase) =>
        phase switch
        {
            BridgeImportPhase.ReadingFiles => "reading_files",
            BridgeImportPhase.MeasuringLoudness => "measuring_loudness",
            BridgeImportPhase.Finalizing => "finalizing",
            _ => throw new ArgumentOutOfRangeException(nameof(phase), phase, "Unknown import phase"),
        };

    private static string ValidationReasonTag(BridgeValidationReason reason) =>
        reason switch
        {
            BridgeValidationReason.EmptyAlbumTitle => "empty_album_title",
            BridgeValidationReason.NoAlbumArtist => "no_album_artist",
            BridgeValidationReason.InvalidYear => "invalid_year",
            _ => throw new ArgumentOutOfRangeException(nameof(reason), reason, "Unknown validation reason"),
        };

    private static BridgeHomeStorage HomeStorage(string storage) =>
        storage == "browsable" ? BridgeHomeStorage.Browsable : BridgeHomeStorage.Opaque;

    private static BridgeStorageMode StorageMode(string storageMode) =>
        storageMode == "cloud" ? BridgeStorageMode.Remote : BridgeStorageMode.Local;

    private static BridgeSignalToggle SignalToggle(string kind, string value) =>
        kind switch
        {
            "disc_id" => new BridgeSignalToggle.Disc(),
            "barcode" => new BridgeSignalToggle.Barcode(),
            _ => new BridgeSignalToggle.Catalog(value),
        };

    private static BridgeReleaseUserEdit ReleaseUserEdit(BridgeRawReleaseEdit edit) =>
        BaeBridgeMethods.ShapeReleaseEdit(edit) switch
        {
            BridgeShapeResult.Valid valid => valid.Edit,
            BridgeShapeResult.Invalid invalid => throw new ArgumentException(
                $"invalid release edit: {ValidationReasonTag(invalid.Reason)}",
                nameof(edit)),
            _ => throw new ArgumentException("invalid release edit", nameof(edit)),
        };

    private static string DiscogsSaveOutcomeTag(BridgeDiscogsSaveOutcome outcome) =>
        outcome switch
        {
            BridgeDiscogsSaveOutcome.Valid => "valid",
            BridgeDiscogsSaveOutcome.Unvalidated => "unvalidated",
            BridgeDiscogsSaveOutcome.Rejected => "rejected",
            _ => throw new ArgumentOutOfRangeException(nameof(outcome), outcome, "Unknown Discogs save outcome"),
        };


}
