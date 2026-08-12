using System.Linq;
using System.Text.Json;
using uniffi.bae_bridge;


namespace Bae.Desktop;

internal static partial class NativeBae
{
    // Start the bridge call on a threadpool thread and block until it finishes.
    // The call must NOT start on the blocking thread: the generated async
    // bindings' internal awaits capture the caller's SynchronizationContext, so
    // a bridge call started on the UI thread posts its completion to the UI
    // dispatcher — which a blocking wait on that same thread starves (observed
    // as a permanent hang opening a library from the welcome chooser). Task.Run
    // starts the call with no context, so completions never need the blocked
    // thread.
    private static T Await<T>(Func<System.Threading.Tasks.Task<T>> call) =>
        System.Threading.Tasks.Task.Run(call).GetAwaiter().GetResult();

    private static void Await(Func<System.Threading.Tasks.Task> call) =>
        System.Threading.Tasks.Task.Run(call).GetAwaiter().GetResult();

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

    private static List<ReleaseCandidateChoice> CandidateChoices(BridgeCandidateSearchResults results) =>
        results.Groups
            .SelectMany(group => group.Pressings.Select(pressing => new ReleaseCandidateChoice(group, pressing)))
            .ToList();

    private static List<LocalArtwork> LocalArtwork(BridgeCandidateFiles files) =>
        files.Files.Select(LocalArtwork).OfType<LocalArtwork>().ToList();

    // The candidate's images, as cover choices. A file that is not an image has
    // no choice to offer, so it drops out.
    private static LocalArtwork? LocalArtwork(BridgeCandidateFile file)
    {
        var choice = file.Role switch
        {
            BridgeFileRole.Cover cover => cover.Choice,
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

    internal static ImportCandidate ImportCandidateRow(
        BridgeFolderCandidate candidate,
        BridgeCandidateRuntimeSnapshot runtime) =>
        new()
        {
            Key = candidate.FolderPath,
            Name = candidate.SourceFolderName,
            TrackCount = checked((int)candidate.TrackCount),
            Format = candidate.Files.FormatLabel,
            RowStatus = ImportRowStatus(runtime),
            Matches = ImportMatches(runtime.IdentifyState),
            Signals = runtime.SignalsToolbar.Signals.Select(SignalBadge).ToList(),
            Files = candidate.Files,
            LocalArtwork = LocalArtwork(candidate.Files),
            FolderPath = candidate.FolderPath,
            Skipped = candidate.Skipped,
            IsAdded = candidate.IsAdded,
        };

    internal static (
        ImportCandidateRowStatus RowStatus,
        List<ReleaseCandidateChoice> Matches,
        List<SignalBadge> Signals) ImportPipeline(BridgeCandidateRuntimeSnapshot runtime) =>
        (
            ImportRowStatus(runtime),
            ImportMatches(runtime.IdentifyState),
            runtime.SignalsToolbar.Signals.Select(SignalBadge).ToList()
        );

    private static ImportCandidateRowStatus ImportRowStatus(BridgeCandidateRuntimeSnapshot runtime)
    {
        if (runtime.ImportStatus is BridgeCandidateImportStatus.Importing importing)
        {
            return new ImportCandidateRowStatus
            {
                Kind = "importing",
                ProgressPercent = checked((int)importing.ProgressPercent),
                Step = importing.Step is null ? null : ImportStep(importing.Step),
            };
        }
        if (runtime.ImportStatus is BridgeCandidateImportStatus.Complete)
        {
            return new ImportCandidateRowStatus { Kind = "complete" };
        }
        if (runtime.ImportStatus is BridgeCandidateImportStatus.Error error)
        {
            return new ImportCandidateRowStatus
            {
                Kind = "error",
                Error = error.ErrorValue,
            };
        }

        return runtime.IdentifyState switch
        {
            BridgeIdentifyState.Idle => new ImportCandidateRowStatus { Kind = string.Empty },
            BridgeIdentifyState.Triangulating => new ImportCandidateRowStatus { Kind = "identifying" },
            BridgeIdentifyState.Found found => new ImportCandidateRowStatus { Kind = "found", Count = checked((int)found.Group.Pressings.Length) },
            BridgeIdentifyState.Conflict => new ImportCandidateRowStatus { Kind = "conflict" },
            BridgeIdentifyState.NotFoundAnywhere => new ImportCandidateRowStatus { Kind = "not_found" },
            BridgeIdentifyState.ManualOnly => new ImportCandidateRowStatus { Kind = "manual" },
            _ => throw new ArgumentOutOfRangeException(nameof(runtime), runtime.IdentifyState, "Unknown identify state"),
        };
    }

    private static List<ReleaseCandidateChoice> ImportMatches(BridgeIdentifyState state) =>
        state is BridgeIdentifyState.Found found
            ? found.Group.Pressings.Select(pressing => new ReleaseCandidateChoice(found.Group, pressing)).ToList()
            : [];

    private static SignalBadge SignalBadge(BridgeToolbarSignal signal) =>
        new()
        {
            Kind = SignalKindTag(signal.Kind),
            Value = signal.Value,
            State = SignalState(signal.State),
            Excluded = signal.Excluded,
        };

    private static SignalBadgeState SignalState(BridgeSignalState state) =>
        state switch
        {
            BridgeSignalState.LookingUp => new() { Kind = "looking_up" },
            BridgeSignalState.Found found => new() { Kind = "found", Count = found.Count },
            BridgeSignalState.NoMatch => new() { Kind = "no_match" },
            BridgeSignalState.Skipped => new() { Kind = "skipped" },
            BridgeSignalState.Failed failed => new() { Kind = "failed", Failure = failed.Failure },
            BridgeSignalState.Confirms confirms => new() { Kind = "confirms", Count = confirms.Count },
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
            BridgePrepareStep.ParsingMetadata => "parsing_metadata",
            BridgePrepareStep.WritingCoverArt => "writing_cover_art",
            BridgePrepareStep.DiscoveringFiles => "discovering_files",
            BridgePrepareStep.ValidatingTracks => "validating_tracks",
            BridgePrepareStep.SavingToDatabase => "saving_to_database",
            _ => throw new ArgumentOutOfRangeException(nameof(step), step, "Unknown prepare step"),
        };

    private static string ImportPhaseTag(BridgeImportPhase phase) =>
        phase switch
        {
            BridgeImportPhase.ReferencingFiles => "referencing_files",
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


    private static BridgeMetadataSource MetadataSource(string source) =>
        source == "discogs" ? BridgeMetadataSource.Discogs : BridgeMetadataSource.MusicBrainz;

    private static BridgeHomeStorage HomeStorage(string storage) =>
        storage == "browsable" ? BridgeHomeStorage.Browsable : BridgeHomeStorage.Opaque;

    private static BridgeStorageMode StorageMode(string storageMode) =>
        storageMode == "managed" ? BridgeStorageMode.Remote : BridgeStorageMode.Local;

    private static BridgeExcludedSignal ExcludedSignal(string kind, string value) =>
        kind switch
        {
            "disc_id" => new BridgeExcludedSignal.Disc(),
            "barcode" => new BridgeExcludedSignal.Barcode(),
            _ => new BridgeExcludedSignal.Catalog(value),
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
