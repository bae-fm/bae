using Xunit;

namespace Bae.Desktop.Tests;

// The settings Remove section's pure decision layer: which footer and
// confirmation-body catalog keys to show, and the pending-cloud-work
// derivation from the outbox snapshot's plain counts.
public sealed class ForgetLibraryModelTests
{
    [Fact]
    public void FooterKey_SplitsOnCloudHome()
    {
        Assert.Equal("settings.remove.footer_synced", ForgetLibraryModel.FooterKey(hasCloudHome: true));
        Assert.Equal("settings.remove.footer_unsynced", ForgetLibraryModel.FooterKey(hasCloudHome: false));
    }

    [Fact]
    public void ConfirmKeys_NeverSynced_IgnoresPendingFlag()
    {
        Assert.Equal(
            new[] { "settings.remove.confirm_unsynced", "settings.remove.confirm_again" },
            ForgetLibraryModel.ConfirmKeys(hasCloudHome: false, hasPendingCloudWork: false));
        Assert.Equal(
            new[] { "settings.remove.confirm_unsynced", "settings.remove.confirm_again" },
            ForgetLibraryModel.ConfirmKeys(hasCloudHome: false, hasPendingCloudWork: true));
    }

    [Fact]
    public void ConfirmKeys_Synced_NoPending()
    {
        Assert.Equal(
            new[] { "settings.remove.confirm_synced", "settings.remove.confirm_again" },
            ForgetLibraryModel.ConfirmKeys(hasCloudHome: true, hasPendingCloudWork: false));
    }

    [Fact]
    public void ConfirmKeys_Synced_WithPending()
    {
        Assert.Equal(
            new[]
            {
                "settings.remove.confirm_synced",
                "settings.remove.confirm_pending",
                "settings.remove.confirm_again",
            },
            ForgetLibraryModel.ConfirmKeys(hasCloudHome: true, hasPendingCloudWork: true));
    }

    [Fact]
    public void HasPendingCloudWork_False_AtZero()
    {
        Assert.False(ForgetLibraryModel.HasPendingCloudWork(uploadGroupCount: 0, pendingDeletes: 0));
    }

    [Fact]
    public void HasPendingCloudWork_True_WithUploadGroupsOnly()
    {
        Assert.True(ForgetLibraryModel.HasPendingCloudWork(uploadGroupCount: 1, pendingDeletes: 0));
    }

    [Fact]
    public void HasPendingCloudWork_True_WithPendingDeletesOnly()
    {
        Assert.True(ForgetLibraryModel.HasPendingCloudWork(uploadGroupCount: 0, pendingDeletes: 1));
    }
}
