using System.Collections.Generic;

namespace Bae.Windows;

// The pure decision layer behind the settings Remove section: which footer and
// confirmation catalog keys to show, mirroring macOS's forgetConfirmationMessage.
// Plain BCL types so it is unit-tested apart from the WinUI dialog that renders
// it and the WithCurrentHandle-read BridgeOutboxSnapshot it derives from.
public static class ForgetLibraryModel
{
    // The gray caption under the section title: which consequence to describe
    // depends only on whether a cloud home is configured.
    public static string FooterKey(bool hasCloudHome) =>
        hasCloudHome ? "settings.remove.footer_synced" : "settings.remove.footer_unsynced";

    // The confirmation body's ordered key list. A never-synced library ignores
    // the pending-work flag (there is no cloud copy to lose work from); a synced
    // library calls out queued-but-unlanded work before the final prompt.
    public static List<string> ConfirmKeys(bool hasCloudHome, bool hasPendingCloudWork)
    {
        if (!hasCloudHome)
        {
            return ["settings.remove.confirm_unsynced", "settings.remove.confirm_again"];
        }

        return hasPendingCloudWork
            ? ["settings.remove.confirm_synced", "settings.remove.confirm_pending", "settings.remove.confirm_again"]
            : ["settings.remove.confirm_synced", "settings.remove.confirm_again"];
    }

    // Whether the outbox snapshot has cloud work that hasn't landed yet: queued
    // uploads or deletes not yet applied to the cloud copy.
    public static bool HasPendingCloudWork(int uploadGroupCount, long pendingDeletes) =>
        uploadGroupCount > 0 || pendingDeletes > 0;
}
