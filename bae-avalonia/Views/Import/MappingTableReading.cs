using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Reading the mapping table. Every value here is either one bae-core already
/// decided — which track a row commits, what its source is, which catalog
/// message names the tally — or a count over the table's own rows. Nothing here
/// works out a pairing.
/// </summary>
internal static class MappingTableReading
{
    /// <summary>The units a track group carries: itself, or the entries a track
    /// sheet carves.</summary>
    internal static IReadOnlyList<BridgeMappingUnit> Units(
        this BridgeMappingTrackGroup group) => group switch
    {
        BridgeMappingTrackGroup.Unit unit => new[] { unit.UnitValue },
        BridgeMappingTrackGroup.Sheet sheet => sheet.Entries,
        _ => throw new ArgumentOutOfRangeException(
            nameof(group), group, "Unknown mapping track group"),
    };

    /// <summary>Every unit the table carries, top-level rows and sheet entries
    /// alike, in the order the table lays them out.</summary>
    internal static IEnumerable<BridgeMappingUnit> Units(this BridgeMappingTable table) =>
        table.TrackGroups.SelectMany(Units);

    /// <summary>The track this row commits, where it commits one.</summary>
    internal static BridgeRawTrackEdit? Track(this BridgeMappingUnit unit) =>
        unit.Becomes is BridgeMappingBecomes.Track track ? track.TrackValue : null;

    /// <summary>Rows that will write a track: the ones carrying audio. A track
    /// the release names that the folder has nothing for writes nothing.</summary>
    internal static int WillWriteCount(this BridgeMappingTable table) =>
        table.Units().Count(unit => unit.Track()?.File is not null);

    /// <summary>Rows that will write a track nobody has named.</summary>
    internal static int UnansweredCount(this BridgeMappingTable table) =>
        table.Units().Count(unit =>
            unit.Track() is { File: not null } track && string.IsNullOrWhiteSpace(track.Title));

    /// <summary>Every audio unit the table's rows carry, in table order — what a
    /// row with nothing behind it is offered to point at.</summary>
    internal static IReadOnlyList<ImportAudioChoice> AudioChoices(this BridgeMappingTable table) =>
        table.Units().Select(AudioChoice).OfType<ImportAudioChoice>().ToList();

    private static ImportAudioChoice? AudioChoice(BridgeMappingUnit unit)
    {
        if (unit.Track()?.File is not { } audio)
        {
            return null;
        }
        return unit.Source switch
        {
            BridgeMappingSource.File file => new ImportAudioChoice(
                audio,
                $"{file.FileValue.Name}  ·  {Loc.Bytes(checked((long)file.FileValue.Size))}"),
            BridgeMappingSource.SheetEntry entry => new ImportAudioChoice(
                audio,
                $"{entry.Entry.ContainerName}  ·  {entry.Entry.Number}"),
            _ => null,
        };
    }

    /// <summary>The file on disk this row's audio lives in — the file itself, or
    /// the container a sheet entry is carved out of. Null for a track the release
    /// names that the folder has nothing for.</summary>
    internal static string? AudioPath(this BridgeMappingSource source) => source switch
    {
        BridgeMappingSource.File file => file.FileValue.LocalPath,
        BridgeMappingSource.SheetEntry entry => entry.Entry.ContainerLocalPath,
        _ => null,
    };

    /// <summary>The playing time the folder itself offers for this row, from
    /// the scan facts or the sheet timing for one of its entries.</summary>
    internal static ulong? DurationMs(this BridgeMappingSource source) => source switch
    {
        BridgeMappingSource.File file => file.FileValue.DurationMs,
        BridgeMappingSource.SheetEntry entry => entry.Entry.DurationMs,
        _ => null,
    };

    /// <summary>The same role the scan proposed, which is what carries the
    /// localization key. A mapping row's role is that role narrowed to the ones a
    /// row can hold, so every case has an exact counterpart.</summary>
    internal static BridgeFileRole FileRole(this BridgeMappingRole role) => role switch
    {
        BridgeMappingRole.Audio => new BridgeFileRole.Audio(),
        BridgeMappingRole.Document => new BridgeFileRole.Document(),
        BridgeMappingRole.Other => new BridgeFileRole.Other(),
        _ => throw new ArgumentOutOfRangeException(nameof(role), role, "Unknown mapping role"),
    };

    /// <summary>The audio a track sheet is on, where it is on any — the file
    /// whose rows the sheet's group stands for.</summary>
    internal static BridgeMappingContainer? Container(this BridgeSheetBound bound) => bound switch
    {
        BridgeSheetBound.Describes describes => describes.Container,
        BridgeSheetBound.RefusedCodec refused => refused.Container,
        _ => null,
    };

    /// <summary>The tally above the table, in the user's language, or null where
    /// there is no line to draw — core says which by naming a key or not, and
    /// two sides that account for the same rows name none. Each message takes
    /// its own numbers, in the order the English value names them.</summary>
    internal static string? ReconciliationLine(BridgeSlotReconciliation reconciliation) =>
        BaeBridgeMethods.BridgeSlotReconciliationKey(reconciliation) is { } key
            ? Loc.Core(key, ReconciliationArgs(reconciliation))
            : null;

    /// <summary>The arguments the reconciliation message interpolates. The key
    /// itself is core's; only which numbers ride with it differs per
    /// variant.</summary>
    internal static IReadOnlyDictionary<string, object?> ReconciliationArgs(
        BridgeSlotReconciliation reconciliation) => reconciliation switch
        {
            // An agreement draws no line, so it interpolates nothing.
            BridgeSlotReconciliation.Agrees => new Dictionary<string, object?>(),
            BridgeSlotReconciliation.MoreFiles more => new Dictionary<string, object?>
            {
                ["files"] = (long)more.Files,
                ["tracks"] = (long)more.Tracks,
            },
            BridgeSlotReconciliation.MoreTracks more => new Dictionary<string, object?>
            {
                ["files"] = (long)more.Files,
                ["tracks"] = (long)more.Tracks,
            },
            _ => throw new ArgumentOutOfRangeException(
                nameof(reconciliation), reconciliation, "Unknown reconciliation"),
        };

    /// <summary>A duration as a clock label, or an em dash where there is no
    /// number. Never a zero: an unknown length and a zero-length file are
    /// different facts, and only one of them is real.</summary>
    internal static string DurationText(ulong? milliseconds) =>
        milliseconds is { } value ? BridgeDisplay.Clock(value) : "—";

    /// <summary>The audio unit a row's samples come from — the identity core
    /// keys a measurement by. Null for a track the release names that the
    /// folder has nothing for.</summary>
    internal static BridgeAudioFile? Audio(this BridgeMappingSource source) => source switch
    {
        BridgeMappingSource.File file => new BridgeAudioFile.Standalone(file.FileValue.FileId),
        BridgeMappingSource.SheetEntry entry => new BridgeAudioFile.SheetSlice(
            entry.Entry.ContainerId, entry.Entry.SheetId, entry.Entry.Index),
        _ => null,
    };
}

/// <summary>One of the folder's audio units as the "choose file" menu offers it:
/// what picking it writes onto a row, and what to call it.</summary>
internal sealed record ImportAudioChoice(BridgeAudioFile Audio, string Label);
