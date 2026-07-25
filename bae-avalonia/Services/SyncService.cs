using System;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Cloud sync connection management, membership, and sync-config writes — the C#
/// mirror of BaeKit's <c>Sync</c> closure-struct, minus the Apple-only CloudKit
/// connect (the Windows bindings don't export that path). Network operations are
/// async off the UI thread and carry the session-swap currency plus the error line
/// or value the bridge surfaces; the local writes are synchronous. Every delegate
/// defaults to a fail-loud stub; <see cref="FromSession"/> is the production
/// wiring. <c>SignInCloudProvider</c> reaches a bridge method compiled in only for
/// the OAuth-provider build; an S3-only build's wrapper throws, as it does today.
/// </summary>
internal sealed class SyncService
{
    public Func<BridgeCloudProvider, BridgeHomeStorage, Task<(bool Current, string? Error)>> SignInCloudProvider { get; init; }
        = (_, _) => throw new InvalidOperationException("SyncService stub: SignInCloudProvider not wired");

    public Func<(bool Current, string? Error)> DisconnectCloudProvider { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: DisconnectCloudProvider not wired");

    public Func<BridgeSaveSyncConfig, Task<(bool Current, string? Error)>> SaveSyncConfig { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: SaveSyncConfig not wired");

    /// <summary>Generate a fresh restore code (returns the code, or the error line).</summary>
    public Func<Task<(bool Current, string? Code)>> GenerateRestoreCode { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: GenerateRestoreCode not wired");

    /// <summary>The library's membership: its devices and whether this device is an
    /// owner. Reads the membership chain from cloud storage.</summary>
    public Func<Task<(bool Current, (BridgeMembership? Membership, string? Error) Result)>> GetMembers { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: GetMembers not wired");

    /// <summary>Approve a joining device by its public key, returning the invite
    /// code to hand back (or the error line). The email is baked in so the approver
    /// can share the OAuth folder.</summary>
    public Func<string, string?, Task<(bool Current, string? InviteCode)>> InviteMember { get; init; }
        = (_, _) => throw new InvalidOperationException("SyncService stub: InviteMember not wired");

    /// <summary>Remove a device from the library and rotate the library key.</summary>
    public Func<string, Task<(bool Current, string? Error)>> RemoveMember { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: RemoveMember not wired");

    /// <summary>How many releases live only in the cloud and would become
    /// unplayable if this device disconnected; 0 means nothing is at risk.</summary>
    public Func<Task<(bool Current, (long? Count, string? Error) Result)>> CloudOnlyReleaseCount { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: CloudOnlyReleaseCount not wired");

    /// <summary>Retry failed cloud-outbox uploads now (clears their backoff and
    /// kicks the sync loop).</summary>
    public Func<Task<(bool Current, string? Error)>> RetryOutbox { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: RetryOutbox not wired");

    /// <summary>Rename a library by id.</summary>
    public Func<string, string, (bool Current, string? Error)> RenameLibrary { get; init; }
        = (_, _) => throw new InvalidOperationException("SyncService stub: RenameLibrary not wired");

    /// <summary>Cancel one queued outbox entry by id (the local file stays).</summary>
    public Func<long, Task<(bool Current, string? Error)>> CancelOutboxItem { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: CancelOutboxItem not wired");

    /// <summary>Cancel whatever transition a release is mid-flight (pin, upload, or
    /// unmanage), leaving it in its prior state.</summary>
    public Func<string, Task<(bool Current, string? Error)>> CancelReleaseTransition { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: CancelReleaseTransition not wired");

    /// <summary>Pause or resume the cloud-upload pipeline.</summary>
    public Func<bool, Task<(bool Current, string? Error)>> SetSyncPaused { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: SetSyncPaused not wired");

    /// <summary>Re-kick the sync loop now (manual retry).</summary>
    public Func<bool> TriggerSync { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: TriggerSync not wired");

    /// <summary>Delete the active library's encryption key from the OS keyring; the
    /// current session keeps working, the next launch lands on unlock.</summary>
    public Func<(bool Current, string? Error)> LockActiveLibrary { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: LockActiveLibrary not wired");

    /// <summary>How many blob uploads the sync drain runs at once (1..8). A
    /// persisted device-local config write.</summary>
    public Func<uint, (bool Current, string? Error)> SetMaxConcurrentUploads { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: SetMaxConcurrentUploads not wired");

    /// <summary>The current sync-status snapshot — the badge state core decides
    /// (error > syncing > synced > idle) plus the error line — for the toolbar
    /// indicator and the sync banner.</summary>
    public Func<(bool Current, (BridgeSyncStatusSnapshot? Status, string? Error) Result)> SyncStatus { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: SyncStatus not wired");

    /// <summary>The cloud-outbox snapshot — the queued/in-flight uploads — for the
    /// storage sheet's per-release upload state and the Exporting band.</summary>
    public Func<Task<(bool Current, (BridgeOutboxSnapshot? Snapshot, string? Error) Result)>> OutboxSnapshot { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: OutboxSnapshot not wired");

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static SyncService FromSession(SessionStore session) => new()
    {
        SignInCloudProvider = (provider, storage) =>
            session.RunForCurrentHandle(handle => NativeBae.SignInCloud(handle, provider, storage)),
        DisconnectCloudProvider = () => session.WithCurrentHandle(NativeBae.DisconnectCloud),
        SaveSyncConfig = config => session.RunForCurrentHandle(handle => NativeBae.SaveSyncConfig(handle, config)),
        GenerateRestoreCode = () => session.RunForCurrentHandle(NativeBae.GenerateRestoreCode),
        GetMembers = () => session.RunForCurrentHandle(NativeBae.GetMembers),
        InviteMember = (publicKeyHex, email) =>
            session.RunForCurrentHandle(handle => NativeBae.InviteMember(handle, publicKeyHex, email)),
        RemoveMember = publicKeyHex =>
            session.RunForCurrentHandle(handle => NativeBae.RemoveMember(handle, publicKeyHex)),
        CloudOnlyReleaseCount = () => session.RunForCurrentHandle(NativeBae.CloudOnlyReleaseCount),
        RetryOutbox = () => session.RunForCurrentHandle(NativeBae.RetryOutbox),
        RenameLibrary = (libraryId, newName) =>
            session.WithCurrentHandle(handle => NativeBae.RenameLibrary(handle, libraryId, newName)),
        CancelOutboxItem = id => session.RunForCurrentHandle(handle => NativeBae.CancelOutboxItem(handle, id)),
        CancelReleaseTransition = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.CancelReleaseTransition(handle, releaseId)),
        SetSyncPaused = paused => session.RunForCurrentHandle(handle => NativeBae.SetSyncPaused(handle, paused)),
        TriggerSync = () => session.WithCurrentHandle(NativeBae.TriggerSync),
        LockActiveLibrary = () => session.WithCurrentHandle(NativeBae.LockActiveLibrary),
        SetMaxConcurrentUploads = n => session.WithCurrentHandle(handle => NativeBae.SetMaxConcurrentUploads(handle, n)),
        SyncStatus = () => session.WithCurrentHandle(NativeBae.SyncStatus),
        OutboxSnapshot = () => session.RunForCurrentHandle(NativeBae.OutboxSnapshot),
    };
}
