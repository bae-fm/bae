package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import uniffi.bae_bridge.BridgeException
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
private object NothingFails : ErrorDisplays {
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
) {
    val people = scenePeople()
    val callbacks = MemberCallbacks({}, { _, _ -> }, { _, _ -> }, {}, {}, {})
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(modifier = Modifier.fillMaxSize()) {
                SettingsTopBar(
                    title =
                        when (page) {
                            PeoplePage.People -> {
                                stringResource(R.string.members_title)
                            }

                            PeoplePage.Devices -> {
                                stringResource(R.string.people_your_devices)
                            }

                            is PeoplePage.Person -> {
                                people
                                    .first { it.membership.id == page.memberId }
                                    .profile
                                    ?.name
                                    .orEmpty()
                            }
                        },
                    onBack = {},
                )
                when (page) {
                    PeoplePage.People -> {
                        PeopleHome(
                            you = YouActions(NothingFails, {}, {}, {}),
                            people = people,
                            selfIsAdmin = asAdmin,
                            requested = null,
                            invites = emptyList(),
                            adminChanges = emptyList(),
                            callbacks = callbacks,
                            inviteCallbacks = InviteCallbacks({}, {}),
                            onOpen = {},
                        )
                    }

                    is PeoplePage.Person -> {
                        PersonPage(
                            member = people.first { it.membership.id == page.memberId }.membership,
                            selfIsAdmin = asAdmin,
                            name = "Person Name",
                            callbacks = callbacks,
                        )
                    }

                    PeoplePage.Devices -> {
                        DevicesPage(member = people.first { it.membership.isSelf }.membership, callbacks = callbacks)
                    }
                }
            }
        }
    }
}

/** The page of the other person, as a preview shows it. */
internal val sceneOtherPerson = PeoplePage.Person(OTHER_ID)

@Preview(showBackground = true)
@Composable
private fun PeopleScenePreview() {
    PeopleScene(PeoplePage.People)
}

@Preview(showBackground = true)
@Composable
private fun PersonScenePreview() {
    PeopleScene(sceneOtherPerson)
}

@Preview(showBackground = true)
@Composable
private fun DevicesScenePreview() {
    PeopleScene(PeoplePage.Devices)
}
