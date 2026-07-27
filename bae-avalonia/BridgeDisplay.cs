using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal static class BridgeDisplay
{
    /// <summary>
    /// The clock label for a raw millisecond duration ("3:07", "1:12:34"), or an
    /// empty string when there is nothing to label — no duration, or a negative
    /// one. Core decides the label's fields; <see cref="Loc.Clock"/> renders them.
    /// </summary>
    internal static string Clock(long? milliseconds) =>
        Render(BaeBridgeMethods.BridgeClock(milliseconds));

    /// <summary>The clock label for an unsigned duration, as playback reports it.</summary>
    internal static string Clock(ulong milliseconds) =>
        Clock(checked((long)milliseconds));

    /// <summary>
    /// The label for a clock core pre-computed onto a row at conversion, or an
    /// empty string when there is nothing to label. A static row carries its
    /// clock as a field rather than paying an FFI call per row to render it.
    /// </summary>
    internal static string Clock(BridgeDurationClock? clock) =>
        Render(clock);

    private static string Render(BridgeDurationClock? clock) =>
        clock is null
            ? string.Empty
            : Loc.Clock(clock.Negative, clock.Hours, clock.Minutes, clock.Seconds);

    /// <summary>
    /// The seek bar's two labels. The leading one is the elapsed position or the
    /// countdown, per <paramref name="showRemaining"/> (the user's config); the
    /// trailing one is the track's total length, empty when it is not known. Which
    /// label is which is core's decision, not the bar's.
    /// </summary>
    internal static (string Leading, string Trailing) SeekBarClocks(
        ulong positionMs,
        ulong durationMs,
        bool showRemaining)
    {
        var clocks = BaeBridgeMethods.BridgeSeekBar(positionMs, durationMs, showRemaining);
        return (Render(clocks.Leading), Render(clocks.Trailing));
    }

    /// <summary>
    /// A total playing time in words — "39 min", "3 hr", "3 hr, 42 min" — or an
    /// empty string when no track reports a length. Core decides which words the
    /// label has (an hour of music is "1 hr", never "1 hr, 0 min"); the
    /// <c>core.duration.*</c> catalog messages own the words and the pattern that
    /// joins them. The hours word is a plural, so the two halves are resolved
    /// separately and then joined.
    /// </summary>
    internal static string DurationUnits(BridgeDurationUnits? units) => units switch
    {
        null => string.Empty,
        BridgeDurationUnits.HoursOnly hoursOnly => HoursText(hoursOnly.Hours),
        BridgeDurationUnits.MinutesOnly minutesOnly => MinutesText(minutesOnly.Minutes),
        BridgeDurationUnits.HoursAndMinutes both => Loc.Core(
            "core.duration.hours_minutes",
            new Dictionary<string, object?>
            {
                ["hours"] = HoursText(both.Hours),
                ["minutes"] = MinutesText(both.Minutes),
            }),
        _ => string.Empty,
    };

    // The count crosses as a long: MessageFormat selects the plural category from
    // a numeric argument, and the hours word inflects (Hebrew's dual drops the
    // numeral entirely).
    private static string HoursText(ulong hours) =>
        Loc.Core("core.duration.hours", "hours", checked((long)hours));

    private static string MinutesText(ulong minutes) =>
        Loc.Core("core.duration.minutes", "minutes", checked((long)minutes));

    /// <summary>
    /// The localized user-facing line for a bridge error, or null when core says
    /// it has none to show — a cancellation. Null, never a fallback: mapping a
    /// cancellation to "core.error.category.internal" read as "an internal error
    /// occurred", which it was not, and callers must drop it rather than show it.
    /// </summary>
    internal static string? LocalizedLine(BridgeException exception)
    {
        var key = BaeBridgeMethods.BridgeErrorLineKey(exception);
        return key is null ? null : Loc.Core(key);
    }

    internal static string LocalizedLine(BridgeLookupFailure failure)
    {
        var key = BaeBridgeMethods.BridgeLookupFailureKey(failure);
        if (key is null)
        {
            return Loc.Core("core.lookup.failure.diagnostic");
        }

        return failure is BridgeLookupFailure.Provider { Status: { } status }
            ? Loc.Core(key, "status", status)
            : Loc.Core(key);
    }

    /// <summary>
    /// The localized user-facing line for a playback failure, or null when there
    /// is none. The actionable cloud-only cases resolve their own keyed line;
    /// everything else defers to the shared bridge-error path, which is also where
    /// "no line at all" comes from.
    /// </summary>
    internal static string? LocalizedLine(BridgePlaybackErrorReason reason)
    {
        var key = BaeBridgeMethods.BridgePlaybackErrorReasonKey(reason);
        if (key is not null)
        {
            return Loc.Core(key);
        }

        return reason is BridgePlaybackErrorReason.Diagnostic diagnostic
            ? LocalizedLine(diagnostic.Error)
            : null;
    }

    /// <summary>
    /// Why a track sheet describes nothing, in the user's language; null when it
    /// describes something. Core owns both the reason and its wording.
    /// </summary>
    internal static string? UnboundSheetLine(BridgeSheetBinding binding) =>
        binding switch
        {
            BridgeSheetBinding.Describes => null,
            BridgeSheetBinding.Unresolved => Loc.Chrome("import.sheet.describes_nothing_yet"),
            BridgeSheetBinding.RefusedCodec refused => Loc.Core(
                BaeBridgeMethods.BridgeSheetRefusedCodecKey(), "codec", refused.Codec),
            _ => throw new ArgumentOutOfRangeException(
                nameof(binding), binding, "Unknown sheet binding"),
        };

    /// <summary>
    /// Why an audio file cannot back a sheet's binding; null when it can. Core
    /// decides the refusal by probing, so no UI reads a codec to work out what
    /// it may offer.
    /// </summary>
    internal static string? RefusalLine(BridgeSheetBindingOffer offer)
    {
        var key = BaeBridgeMethods.BridgeSheetBindingOfferKey(offer);
        if (key is null)
        {
            return null;
        }

        return offer is BridgeSheetBindingOffer.RefusedCodec refused
            ? Loc.Core(key, "codec", refused.Codec)
            : Loc.Core(key);
    }

    internal static string LocalizedLine(BridgeInvalidReason reason)
    {
        var key = BaeBridgeMethods.BridgeInvalidReasonKey(reason);
        return reason switch
        {
            BridgeInvalidReason.CorruptAudioFile file => Loc.Core(key, "path", file.Path),
            BridgeInvalidReason.CorruptImage image => Loc.Core(key, "path", image.Path),
            _ => Loc.Core(key),
        };
    }

    /// <summary>
    /// A triage row's disagreement sentence, resolved from the <c>Core</c>
    /// catalog via the key bae-core owns for this variant. Every number crosses
    /// raw — the enum's own operands — so this is the one place they're
    /// interpolated for the current locale. Durations go through
    /// <see cref="Clock(ulong)"/> first, matching every other duration in the
    /// app; <c>ToleranceMs</c> never appears in the sentence even though
    /// <c>DurationsDisagree</c> carries it — bae-core's own doc on the variant
    /// explains why.
    /// </summary>
    internal static string LocalizedLine(BridgeNeedsYou needsYou)
    {
        var key = BaeBridgeMethods.BridgeNeedsYouKey(needsYou);
        return needsYou switch
        {
            BridgeNeedsYou.SeveralMatches several => Loc.Core(key, "count", (long)several.Count),
            BridgeNeedsYou.TrackCountDisagrees counts => Loc.Core(
                key,
                new Dictionary<string, object?> { ["local"] = (long)counts.Local, ["source"] = (long)counts.Source }),
            BridgeNeedsYou.DurationsDisagree durations => Loc.Core(
                key,
                new Dictionary<string, object?>
                {
                    ["probed"] = Clock(durations.ProbedMs),
                    ["source"] = Clock(durations.SourceMs),
                }),
            _ => Loc.Core(key),
        };
    }

    /// <summary>
    /// The still-identifying row's sub-line. UI-owned chrome, not a
    /// <c>Core</c> catalog key: the phase names three different kinds of "no
    /// verdict yet," and none of them is a sentence bae-core authored — it only
    /// hands over which of the three this candidate is in.
    /// </summary>
    /// <summary>The localized label for a Done row's in-progress import step —
    /// a raw <c>BridgeImportStep</c> off <c>BridgeTriageRow.ImportStatus</c>,
    /// not the string-tag round trip <see cref="PrepareStepKey"/>/
    /// <see cref="ImportPhaseKey"/> exist for (those serve the older
    /// string-keyed <c>ImportCandidateRowStatus</c> the picker dialog still
    /// uses).</summary>
    internal static string LocalizedLine(BridgeImportStep step) => step switch
    {
        BridgeImportStep.Preparing preparing => Loc.Core(BaeBridgeMethods.BridgePrepareStepKey(preparing.Step)),
        BridgeImportStep.Running running => Loc.Core(BaeBridgeMethods.BridgeImportPhaseKey(running.Phase)),
        _ => string.Empty,
    };

    internal static string LocalizedLine(BridgeIdentifyPhase phase) => phase switch
    {
        BridgeIdentifyPhase.Queued => Loc.Chrome("import.phase.queued"),
        BridgeIdentifyPhase.Running => Loc.Chrome("import.phase.running"),
        BridgeIdentifyPhase.NoAnswer => Loc.Chrome("import.phase.no_answer"),
        _ => string.Empty,
    };

    // The wire provider tag (google_drive / dropbox / onedrive / s3) as a name to
    // show the user. Unknown tags pass through unchanged.
    internal static string ProviderDisplayName(string provider) => provider switch
    {
        "google_drive" => Loc.Chrome("cloud.provider.google_drive"),
        "dropbox" => Loc.Chrome("cloud.provider.dropbox"),
        "onedrive" => Loc.Chrome("cloud.provider.onedrive"),
        "s3" => Loc.Chrome("cloud.provider.s3"),
        _ => provider,
    };

    internal static string ProviderDisplayName(BridgeCloudProvider provider)
    {
        var key = NativeBae.CloudProviderLabelKey(provider);
        if (key is not null)
        {
            return Loc.Core(key);
        }

        return provider switch
        {
            BridgeCloudProvider.GoogleDrive => Loc.Chrome("cloud.provider.google_drive"),
            BridgeCloudProvider.Dropbox => Loc.Chrome("cloud.provider.dropbox"),
            BridgeCloudProvider.OneDrive => Loc.Chrome("cloud.provider.onedrive"),
            BridgeCloudProvider.CloudKit => Loc.Chrome("cloud.provider.icloud"),
            _ => provider.ToString(),
        };
    }

    // The stable wire token for a release-storage transition (pin/unpin/manage/
    // unmanage), core's mapping — the key the transfer-progress store files an
    // in-flight transition under so its matching "ended" event clears the right
    // one. Handle-less: it is a pure enum → token translation, not a session read.
    internal static string TransferActionToken(BridgeReleaseStorageAction action) =>
        NativeBae.TransferActionToken(action);

    // The catalog key for a transition's in-flight progress verb, from its wire
    // token (pin/unpin/manage/unmanage), or null for a token core doesn't name.
    // The storage cell shows this verb while a release's transfer runs, whether the
    // token comes from the live overlay or the row's own reported transfer action.
    internal static string? TransferVerbKey(string token) =>
        NativeBae.TransferActionKey(token);

    // The localization key for a cloud provider's label from its wire tag, or null
    // when the tag isn't one core names (the caller falls back to chrome).
    internal static string? CloudProviderLabelKey(string? provider) =>
        NativeBae.CloudProviderLabelKey(provider);

    // The localization key for an import candidate's preparing-step tag.
    internal static string? PrepareStepKey(string step) => NativeBae.PrepareStepKey(step);

    // The localization key for an import candidate's running-phase tag.
    internal static string? ImportPhaseKey(string phase) => NativeBae.ImportPhaseKey(phase);
}
