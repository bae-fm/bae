using uniffi.bae_bridge;

namespace Bae.Desktop;

internal static partial class NativeBae
{
    internal static (BridgeEvidenceContent[]? Contents, string? Error) ReadEvidence(
        AppHandle handle, BridgeEvidenceSubject subject, BridgeEvidenceSelection selection) =>
        CaptureBridgeValue(() => Await(() => handle.ReadEvidence(subject, selection)));
}
