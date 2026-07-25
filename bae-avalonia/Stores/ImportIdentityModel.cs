using System;

namespace Bae.Desktop;

// The identity-claim state behind the import confirmation and re-identify
// dialogs: whether the claim is backed by a picked source release, and — when
// it is — whether the user claims the exact pressing or just the album group
// (metadata only). Unknown (no source release) has no exactness choice at all.
// Pure over bools so it is unit-tested apart from the WinUI dialogs and the
// generated bridge identity type.
public sealed class ImportIdentityModel
{
    public bool HasSourceRelease { get; }
    public bool MetadataOnly { get; private set; }

    public ImportIdentityModel(bool hasSourceRelease) { HasSourceRelease = hasSourceRelease; }

    // Absolute set (not a toggle). A claim without a source release has no
    // exactness to choose — calling this then is a caller bug, so fail loud.
    public void SetMetadataOnly(bool metadataOnly)
    {
        if (!HasSourceRelease)
        {
            throw new InvalidOperationException("no source release to claim exactness against");
        }
        MetadataOnly = metadataOnly;
    }

    public bool ShowsExactnessChoice => HasSourceRelease;
    public bool ShowsMetadataOnlyNote => HasSourceRelease && MetadataOnly;
    // Approximate blanks the pressing fields core-side; the form disables them
    // so the blanking is visible. Unknown keeps them editable (file tags may
    // be wrong or absent).
    public bool PressingFieldsEnabled => !(HasSourceRelease && MetadataOnly);
}
