using uniffi.bae_bridge;

namespace Bae.Desktop;

internal sealed class EvidenceService
{
    public Func<BridgeEvidenceSubject, BridgeEvidenceSelection,
        Task<(bool Current, (BridgeEvidenceContent[]? Contents, string? Error) Result)>> Read
    { get; init; }
        = (_, _) => throw new InvalidOperationException("EvidenceService.Read is not wired");

    public static EvidenceService FromSession(SessionStore session) => new()
    {
        Read = (subject, selection) => session.RunForCurrentHandle(
            handle => NativeBae.ReadEvidence(handle, subject, selection)),
    };
}
