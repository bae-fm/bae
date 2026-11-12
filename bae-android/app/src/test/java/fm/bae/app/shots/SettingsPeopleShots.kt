package fm.bae.app.shots

import fm.bae.app.ui.settings.addDeviceParts
import fm.bae.app.ui.settings.cutOffRetryParts
import fm.bae.app.ui.settings.joinRequestParts
import org.junit.Test
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeAdminChangeState
import uniffi.bae_bridge.BridgeDeviceCutOff
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeRestoreCodeShare

/** The shots of the ready android.settings.people*, person*, devices* and add-device* boards: the pages of People and devices. */
class SettingsPeopleShots : BoardShot() {
    /** People and devices as an admin sees it: Ana's own card, Ben, and Invite. */
    @Test
    fun androidSettingsPeople() =
        shot("android.settings.people") {
            ScreenScene { PeoplePageScene(listOf(ana(), ben()), invites = listOf(invitation("another@example.com", waitingCode))) }
        }

    /** People and devices as a member sees it: Ben is an admin, and nothing opens or invites. */
    @Test
    fun androidSettingsPeopleMember() =
        shot("android.settings.people.member") {
            ScreenScene { PeoplePageScene(listOf(ana(BridgeMemberRole.MEMBER), ben(BridgeMemberRole.ADMIN)), selfIsAdmin = false) }
        }

    /** Dan, invited, whose invite shows a code waiting for his request. */
    @Test
    fun androidSettingsPeopleInviteListed() =
        shot("android.settings.people.invite-listed") {
            ScreenScene { PeoplePageScene(listOf(ana(), ben()), invites = listOf(danInvite(waitingCode))) }
        }

    /** Dan's invite while sync is still granting his access. */
    @Test
    fun androidSettingsPeopleInviteWaiting() =
        shot("android.settings.people.invite-waiting") {
            ScreenScene { PeoplePageScene(listOf(ana(), ben()), invites = listOf(danInvite(BridgeInviteShown.Granting))) }
        }

    /** Dan's invite with the reason coven holds it. */
    @Test
    fun androidSettingsPeopleInviteFailed() =
        shot("android.settings.people.invite-failed") {
            ScreenScene { PeoplePageScene(listOf(ana(), ben()), invites = listOf(danInvite(waitingCode, held = heldRejected))) }
        }

    /** Ben's page as an admin sees it: his role with Make admin, Remove from library, his device. */
    @Test
    fun androidSettingsPerson() = shot("android.settings.person") { ScreenScene { PersonPageScene(ben()) } }

    /** Ben's page when he is an admin: his role with Make member. */
    @Test
    fun androidSettingsPersonAdmin() =
        shot("android.settings.person.admin") { ScreenScene { PersonPageScene(ben(BridgeMemberRole.ADMIN)) } }

    /** Ben's page while his promotion waits for sync: the change's box above his role, still Member. */
    @Test
    fun androidSettingsPersonMakeAdminWaiting() =
        shot("android.settings.person.make-admin-waiting") {
            ScreenScene { PersonPageScene(ben(), listOf(makeBenAdmin(BridgeAdminChangeState.Waiting))) }
        }

    /** Ben's page once the library has the promotion: the box says Done with Dismiss, and his role is Admin. */
    @Test
    fun androidSettingsPersonMakeAdminDone() =
        shot("android.settings.person.make-admin-done") {
            ScreenScene { PersonPageScene(ben(BridgeMemberRole.ADMIN), listOf(makeBenAdmin(BridgeAdminChangeState.Done))) }
        }

    /** Ana's own devices: this phone, her laptop, Add device, and her removed devices. */
    @Test
    fun androidSettingsDevices() =
        shot("android.settings.devices") { ScreenScene { DevicesPageScene(ana(formerDevices = removedDevices)) } }

    /** The removal of a device hasn't reached this device yet. */
    @Test
    fun androidSettingsDevicesCutOffNotHere() =
        shot("android.settings.devices.cut-off-not-here") {
            InPlaceDialogScene(behind = { DevicesPageScene(ana()) }) {
                cutOffRetryParts(BridgeDeviceCutOff.DEVICE_NOT_GONE, onTryAgain = {}, onDismiss = {})
            }
        }

    /** The cut-off was refused because this device is still syncing. */
    @Test
    fun androidSettingsDevicesCutOffRunning() =
        shot("android.settings.devices.cut-off-running") {
            InPlaceDialogScene(behind = { DevicesPageScene(ana()) }) {
                cutOffRetryParts(BridgeDeviceCutOff.PROVIDER_REQUEST_RUNNING, onTryAgain = {}, onDismiss = {})
            }
        }

    /** Add device showing the code to scan or type on the new device. */
    @Test
    fun androidSettingsAddDevice() =
        shot("android.settings.add-device") {
            InPlaceDialogScene(behind = { DevicesPageScene(ana()) }) {
                addDeviceParts(share = restoreCodeShare, added = null, error = null, onDismiss = {})
            }
        }

    /** Add device after the new device joined: the tablet is in the list. */
    @Test
    fun androidSettingsAddDeviceAdded() =
        shot("android.settings.add-device.added") {
            val tablet = BridgeActiveDevice(6uL, "Tablet", isSelf = false, canRemove = true)
            InPlaceDialogScene(behind = { DevicesPageScene(ana(devices = ana().membership.activeDevices + tablet)) }) {
                addDeviceParts(share = null, added = tablet, error = null, onDismiss = {})
            }
        }

    /** The request of a new device to join, with the number to compare. */
    @Test
    fun androidSettingsAddDeviceRequest() =
        shot("android.settings.add-device.request") {
            InPlaceDialogScene(behind = { DevicesPageScene(ana()) }) {
                joinRequestParts(name = "Tablet", confirmation = "412 778", onApprove = {}, onDecline = {})
            }
        }
}

private val restoreCodeShare =
    BridgeRestoreCodeShare(
        code = "CVR1 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567",
        qrText = "CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        qrCarriesS3Key = false,
    )
