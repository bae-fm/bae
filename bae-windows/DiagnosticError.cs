using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace Bae.Windows;

/// <summary>
/// A user-facing error from the core, mirroring the FFI's <c>FfiError</c> (and
/// the bridge's <c>BridgeError</c> / macOS's <c>BridgeError+Localized</c>). The
/// locale never crosses the bridge: this is a typed reason plus, for
/// diagnostics, the opaque Rust error chain (<see cref="Detail"/>) the UI logs
/// and offers in a copyable disclosure but never translates. The
/// <see cref="LocalizedLine"/> resolves the generic per-category / per-entity
/// line for the current locale.
/// </summary>
public sealed class DiagnosticError
{
    /// <summary>"not_found" / "diagnostic".</summary>
    public string Kind { get; set; } = "diagnostic";

    /// <summary>The missing entity's wire tag for the "not_found" case
    /// ("library"/"album"/"release"/"track"/"file").</summary>
    public string? Entity { get; set; }

    /// <summary>The missing entity's id for the "not_found" case (log/detail
    /// only — never shown as primary copy).</summary>
    public string? Id { get; set; }

    /// <summary>The diagnostic category wire tag for the "diagnostic" case
    /// ("database"/"config"/"internal"/"import"/"export").</summary>
    public string? Category { get; set; }

    /// <summary>The opaque Rust error chain for the "diagnostic" case — logged
    /// and offered in a copyable disclosure, never translated.</summary>
    public string? Detail { get; set; }

    /// <summary>
    /// The generic localized line for this error. For "not_found" it's the
    /// entity's "… not found" line; for "diagnostic" it's the category's generic
    /// line. The key comes from the FFI (one source for the mapping); a missing
    /// mapping falls back to the internal-error line.
    /// </summary>
    [JsonIgnore]
    public string LocalizedLine
    {
        get
        {
            var key = Kind switch
            {
                "not_found" when Entity is not null => NativeBae.EntityNotFoundKey(Entity),
                "diagnostic" when Category is not null => NativeBae.ErrorCategoryKey(Category),
                _ => null,
            };
            return key is null ? Loc.Core("core.error.category.internal") : Loc.Core(key);
        }
    }
}

/// <summary>
/// Why playback couldn't start or continue, mirroring the FFI's
/// <c>FfiPlaybackErrorReason</c> (and the bridge's
/// <c>BridgePlaybackErrorReason</c>). The actionable cloud-only case is keyed;
/// every in-core failure rides in <see cref="Error"/> and renders through the
/// diagnostic-error path.
/// </summary>
public sealed class PlaybackErrorReason
{
    /// <summary>"sync_disconnected" / "diagnostic".</summary>
    public string Kind { get; set; } = "diagnostic";

    /// <summary>The structured diagnostic for the "diagnostic" case.</summary>
    public DiagnosticError? Error { get; set; }

    /// <summary>The localized line for this reason: the actionable line for the
    /// keyed cases, else the diagnostic error's generic line.</summary>
    [JsonIgnore]
    public string LocalizedLine
    {
        get
        {
            var key = NativeBae.PlaybackErrorReasonKey(Kind);
            if (key is not null)
            {
                return Loc.Core(key);
            }
            return Error?.LocalizedLine ?? Loc.Core("core.error.category.internal");
        }
    }
}

/// <summary>
/// A metadata-lookup failure, mirroring the FFI's <c>FfiLookupFailure</c> (and
/// the bridge's <c>BridgeLookupFailure</c>). The UI resolves a localized line
/// per variant and renders <c>provider</c>'s status as the message argument;
/// <see cref="Detail"/> for the diagnostic case is opaque, log-only.
/// </summary>
public sealed class LookupFailure
{
    /// <summary>"network" / "provider" / "timeout" / "diagnostic".</summary>
    public string Kind { get; set; } = "diagnostic";

    /// <summary>The HTTP status code for the "provider" case, when one was
    /// observed; null otherwise.</summary>
    public int? Status { get; set; }

    /// <summary>The opaque error chain for the "diagnostic" case — log-only.</summary>
    public string? Detail { get; set; }

    /// <summary>The localized failure line for the current locale.</summary>
    [JsonIgnore]
    public string LocalizedLine => Kind switch
    {
        "network" => Loc.Core("core.lookup.failure.network"),
        "timeout" => Loc.Core("core.lookup.failure.timeout"),
        "provider" => Status is { } status
            ? Loc.Core("core.lookup.failure.provider", "status", status)
            : Loc.Core("core.lookup.failure.provider_unknown"),
        // Diagnostic carries no translated copy — show the generic line; Detail
        // is surfaced separately (log / copyable disclosure).
        _ => Loc.Core("core.lookup.failure.diagnostic"),
    };
}
