package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.PreviewData
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInviteEntry
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

private const val SELF_ID = "00000000-0000-4000-8000-000000000001"
private const val OTHER_ID = "00000000-0000-4000-8000-000000000002"

/** The people a preview or a screenshot shows: this person, and one other, who is a member. */
private fun scenePeople(): List<BridgePerson> {
    val me = PreviewData.person(PreviewData.member(id = SELF_ID))
    val other = PreviewData.person(PreviewData.member(id = OTHER_ID, role = BridgeMemberRole.MEMBER, isSelf = false))
    return listOf(
        me.copy(profile = me.profile?.copy(name = "Your Name")),
        other.copy(profile = other.profile?.copy(name = "Person Name")),
    )
}

/** Nothing a scene does fails, so no failure shows. */
internal object NothingFails : ErrorDisplays {
    override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

    override fun display(error: BridgeException): DisplayError? = null

    override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

    override fun display(outcome: BridgePlayOutcome): DisplayError? = null
}

/** A page of People and devices as a preview or a screenshot shows it, over the people above. */
@Composable
internal fun PeopleScene(
    page: PeoplePage,
    asAdmin: Boolean = true,
    invites: List<BridgeInviteEntry> = emptyList(),
) {
    val people = scenePeople()
    val callbacks = MemberCallbacks({}, { _, _ -> }, { _, _ -> }, { _, _ -> }, {}, {})
    when (page) {
        PeoplePage.People -> {
            AndroidSettingsPeople(
                you = YouActions(NothingFails, {}, {}, {}),
                people = people,
                selfIsAdmin = asAdmin,
                requested = null,
                invites = invites,
                adminChanges = emptyList(),
                callbacks = callbacks,
                inviteCallbacks = InviteCallbacks({}, {}, {}),
                error = null,
                onOpen = {},
                onBack = {},
            )
        }

        is PeoplePage.Person -> {
            val person = people.first { it.membership.id == page.memberId }
            AndroidSettingsPerson(
                member = person.membership,
                selfIsAdmin = asAdmin,
                name = "Person Name",
                callbacks = callbacks,
                error = null,
                onBack = {},
            )
        }

        PeoplePage.Devices -> {
            AndroidSettingsDevices(
                member = people.first { it.membership.isSelf }.membership,
                callbacks = callbacks,
                error = null,
                onBack = {},
            )
        }
    }
}

/** The page of the other person, as a preview shows it. */
internal val sceneOtherPerson = PeoplePage.Person(OTHER_ID)

@Preview(showBackground = true)
@Composable
private fun PeopleScenePreview() {
    BaeAppChrome { PeopleScene(PeoplePage.People) }
}

@Preview(showBackground = true)
@Composable
private fun PersonScenePreview() {
    BaeAppChrome { PeopleScene(sceneOtherPerson) }
}

@Preview(showBackground = true)
@Composable
private fun DevicesScenePreview() {
    BaeAppChrome { PeopleScene(PeoplePage.Devices) }
}
