package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidScreen
import uniffi.bae_bridge.BridgePerson

// The pages of People and devices as the screen shows them once it has read the people.

/** The page [page] names, over the [people] read. */
@Composable
internal fun LoadedPeoplePage(
    session: OpenLibrary,
    model: MembersModel,
    people: List<BridgePerson>,
    callbacks: MemberCallbacks,
    dialogs: MembersDialogs,
    page: PeoplePage,
    onOpen: (PeoplePage) -> Unit,
    onBack: () -> Unit,
) {
    when (page) {
        PeoplePage.People -> {
            AndroidSettingsPeople(
                you = youActions(session),
                people = people,
                selfIsAdmin = model.selfIsAdmin,
                requested = model.requestedProfile,
                invites = model.inviteEntries,
                adminChanges = model.adminChanges,
                callbacks = callbacks,
                inviteCallbacks = inviteCallbacks(session, model, dialogs),
                error = model.error,
                onOpen = onOpen,
                onBack = onBack,
            )
        }

        is PeoplePage.Person -> {
            val person = people.firstOrNull { it.membership.id == page.memberId }
            if (person == null) {
                PeopleFrame(title = stringResource(R.string.members_unnamed), error = model.error, onBack = onBack) {}
            } else {
                AndroidSettingsPerson(
                    member = person.membership,
                    selfIsAdmin = model.selfIsAdmin,
                    name = person.profile?.name ?: stringResource(R.string.members_unnamed),
                    adminChanges = model.adminChanges,
                    callbacks = callbacks,
                    error = model.error,
                    onBack = onBack,
                )
            }
        }

        PeoplePage.Devices -> {
            val person = people.firstOrNull { it.membership.isSelf }
            if (person == null) {
                PeopleFrame(
                    title = stringResource(R.string.people_your_devices),
                    error = model.error,
                    onBack = onBack,
                ) {}
            } else {
                AndroidSettingsDevices(
                    member = person.membership,
                    callbacks = callbacks,
                    error = model.error,
                    onBack = onBack,
                )
            }
        }
    }
}

/** A page of People and devices: its bar with the [title], a failure of the last change above [content]. */
@Composable
internal fun PeopleFrame(
    title: String,
    error: DisplayError?,
    onBack: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    AndroidScreen(title = title, onBack = onBack, modifier = Modifier.fillMaxSize()) {
        error?.let { shown ->
            AndroidNotice(
                error = shown,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
            )
        }
        content()
    }
}

@Composable
internal fun pageTitle(
    page: PeoplePage,
    read: PeopleRead,
): String =
    when (page) {
        PeoplePage.People -> {
            stringResource(R.string.members_title)
        }

        PeoplePage.Devices -> {
            stringResource(R.string.people_your_devices)
        }

        is PeoplePage.Person -> {
            (read as? PeopleRead.Loaded)
                ?.people
                ?.firstOrNull { it.membership.id == page.memberId }
                ?.profile
                ?.name ?: stringResource(R.string.members_unnamed)
        }
    }

internal fun youActions(session: OpenLibrary): YouActions =
    YouActions(
        errors = session.errors,
        rename = { name -> session.appHandle.renameProfile(name) },
        setHistory = { chosen ->
            session.messages.perform("set history sharing to $chosen") { session.appHandle.setHistorySharing(chosen) }
        },
        createProfile = { name ->
            session.messages.perform("create the person's profile") { session.appHandle.createProfile(name) }
        },
    )

internal fun inviteCallbacks(
    session: OpenLibrary,
    model: MembersModel,
    dialogs: MembersDialogs,
): InviteCallbacks =
    InviteCallbacks(
        onOpen = dialogs::open,
        // Retry now reads a request that has arrived at once, instead of at the next check.
        onRefresh = { session.messages.startRetrySync(session.appHandle::retryNow) },
        onDismissRefused = { refused ->
            model.perform("dismiss a refused invite") { session.appHandle.dismissRefusedInvite(refused.operation) }
        },
    )
