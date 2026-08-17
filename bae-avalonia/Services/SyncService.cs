using System;
using System.Collections.Generic;
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
    /// <summary>Run the OAuth browser sign-in for a provider (wire tag) at a home
    /// storage mode (<c>opaque</c> / <c>browsable</c>), connecting it as the sync
    /// home. The bridge maps the tags to its enums.</summary>
    public Func<string, string, Task<(bool Current, string? Error)>> SignInCloudProvider { get; init; }
        = (_, _) => throw new InvalidOperationException("SyncService stub: SignInCloudProvider not wired");

    public Func<Task<(bool Current, string? Error)>> DisconnectCloudProvider { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: DisconnectCloudProvider not wired");

    /// <summary>Connect an S3-compatible bucket as the sync home: the bucket probe
    /// runs before the write. The bridge maps the storage-mode tag to its enum.</summary>
    public Func<string, string, string, string, string, string, string, Task<(bool Current, string? Error)>> SaveSyncConfig { get; init; }
        = (_, _, _, _, _, _, _) => throw new InvalidOperationException("SyncService stub: SaveSyncConfig not wired");

    /// <summary>Generate a fresh restore code (returns the code, or the error line).</summary>
    public Func<Task<(bool Current, string? Code)>> GenerateRestoreCode { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: GenerateRestoreCode not wired");

    /// <summary>The library's membership: its devices and whether this device is an
    /// owner. Reads the membership chain from cloud storage.</summary>
    public Func<Task<(bool Current, (BridgeMembership? Membership, string? Error) Result)>> GetMembers { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: GetMembers not wired");

    public Func<Task<(bool Current, (BridgeDevicePairingSession? Session, string? Error) Result)>> StartDevicePairing { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: StartDevicePairing not wired");

    public Func<BridgeDevicePairingSession, Task<(bool Current, (BridgePairingDevice? Device, string? Error) Result)>> WaitForPairingDevice { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: WaitForPairingDevice not wired");

    public Func<BridgeDevicePairingSession, Task<(bool Current, string? Error)>> ApprovePairingDevice { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: ApprovePairingDevice not wired");

    public Func<BridgeDevicePairingSession, (bool Current, string? Error)> CancelDevicePairing { get; init; }
        = _ => throw new InvalidOperationException("SyncService stub: CancelDevicePairing not wired");

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
    public Func<Task<(bool Current, string? Error)>> LockActiveLibrary { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: LockActiveLibrary not wired");

    /// <summary>Remove the active library from this device: delete its local data
    /// directory, clear the active-library pointer, and drop its encryption key. Any
    /// cloud copy is untouched.</summary>
    public Func<Task<(bool Current, string? Error)>> ForgetLibrary { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: ForgetLibrary not wired");

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

    /// <summary>The cloud providers this build's native library links, as wire tags
    /// (<c>s3</c>, <c>google_drive</c>, …). An S3-only build returns just S3, so the
    /// settings cloud section offers no OAuth sign-in. Handle-less.</summary>
    public Func<IReadOnlyList<string>> AvailableCloudProviders { get; init; }
        = () => throw new InvalidOperationException("SyncService stub: AvailableCloudProviders not wired");

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static SyncService FromSession(SessionStore session) => new()
    {
        SignInCloudProvider = (provider, storage) =>
            session.RunForCurrentHandle(handle => NativeBae.SignInCloud(handle, provider, storage)),
        DisconnectCloudProvider = () => session.RunForCurrentHandle(NativeBae.DisconnectCloud),
        SaveSyncConfig = (bucket, region, endpoint, keyPrefix, accessKey, secretKey, storage) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.SaveSyncConfig(handle, bucket, region, endpoint, keyPrefix, accessKey, secretKey, storage)),
        GenerateRestoreCode = () => session.RunForCurrentHandle(NativeBae.GenerateRestoreCode),
        GetMembers = () => session.RunForCurrentHandle(NativeBae.GetMembers),
        StartDevicePairing = () => session.RunForCurrentHandle(NativeBae.StartDevicePairing),
        WaitForPairingDevice = pairing =>
            session.RunForCurrentHandle(_ => NativeBae.WaitForPairingDevice(pairing)),
        ApprovePairingDevice = pairing =>
            session.RunForCurrentHandle(_ => NativeBae.ApprovePairingDevice(pairing)),
        CancelDevicePairing = pairing =>
            session.WithCurrentHandle(_ => NativeBae.CancelDevicePairing(pairing)),
        RemoveMember = publicKeyHex =>
            session.RunForCurrentHandle(handle => NativeBae.RemoveMember(handle, publicKeyHex)),
        CloudOnlyReleaseCount = () => session.RunForCurrentHandle(NativeBae.CloudOnlyReleaseCount),
        RetryOutbox = () => session.RunForCurrentHandle(NativeBae.RetryOutbox),
        RenameLibrary = (libraryId, newName) =>
            session.WithCurrentHandle(handle => NativeBae.RenameLibrary(handle, libraryId, newName)),
        CancelReleaseTransition = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.CancelReleaseTransition(handle, releaseId)),
        SetSyncPaused = paused => session.RunForCurrentHandle(handle => NativeBae.SetSyncPaused(handle, paused)),
        TriggerSync = () => session.WithCurrentHandle(NativeBae.TriggerSync),
        LockActiveLibrary = () => session.RunForCurrentHandle(NativeBae.LockActiveLibrary),
        ForgetLibrary = () => session.RunForCurrentHandle(NativeBae.ForgetLibrary),
        SetMaxConcurrentUploads = n => session.WithCurrentHandle(handle => NativeBae.SetMaxConcurrentUploads(handle, n)),
        SyncStatus = () => session.WithCurrentHandle(NativeBae.SyncStatus),
        OutboxSnapshot = () => session.RunForCurrentHandle(NativeBae.OutboxSnapshot),
        AvailableCloudProviders = NativeBae.AvailableCloudProviders,
    };
}
