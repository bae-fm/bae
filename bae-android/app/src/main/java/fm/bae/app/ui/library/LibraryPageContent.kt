package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshState
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.BrowserPageState
import fm.bae.app.data.ListPageState
import fm.bae.app.data.PageError
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidContentUnavailable
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.settings.syncLineText
import kotlinx.coroutines.launch

/**
 * A library browse tab's body: pull-to-refresh over a failed read in the rows' place, nothing
 * until the list's count returns, empty message (with the [emptyAction] under it,
 * when the list has one) or rows, each inside a scrollable so the pull reaches
 * [PullToRefreshBox]. The pull retries sync, and its spinner stays, over the
 * sync status line that says what sync is doing, until the pass the retry ran
 * ends.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibraryPageContent(
    session: OpenLibrary,
    page: BrowserPageState,
    emptyMessage: String,
    emptyAction: (@Composable () -> Unit)? = null,
    rows: @Composable () -> Unit,
) {
    var refreshing by remember { mutableStateOf(false) }
    val refreshScope = rememberScreenScope()
    val onRefresh: () -> Unit = {
        refreshing = true
        refreshScope.launch {
            try {
                session.messages.retrySync(session.appHandle::retryNow)
            } finally {
                refreshing = false
            }
        }
    }
    val state = rememberPullToRefreshState()
    PullToRefreshBox(
        isRefreshing = refreshing,
        onRefresh = onRefresh,
        modifier = Modifier.fillMaxSize(),
        state = state,
        indicator = {
            RetryIndicator(
                session = session,
                state = state,
                refreshing = refreshing,
                modifier = Modifier.align(Alignment.TopCenter),
            )
        },
    ) {
        when (val shown = page.state) {
            is ListPageState.Failed -> {
                LibraryReadFailed(shown.error)
            }

            // The count is a local read, so the screen shows nothing until it returns.
            ListPageState.Waiting -> {
                ListPlaceholder {}
            }

            is ListPageState.Counted -> {
                if (shown.rows == 0) {
                    ListPlaceholder { AndroidContentUnavailable(emptyMessage, actions = emptyAction) }
                } else {
                    rows()
                }
            }
        }
    }
}

/** Shows the centred notice in the list's place when bae could not read the list, with Retry reopening its reads. */
@Composable
internal fun LibraryReadFailed(pageError: PageError) {
    ListPlaceholder {
        AndroidNotice(
            title = stringResource(R.string.library_read_failed),
            advice = null,
            error = pageError.error,
            look = NoticeLook.CENTERED,
            action = pageError.error.retryAction(onClick = pageError.onRetry),
        )
    }
}

/** The pull's spinner, and while the retry runs, the sync status line under it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RetryIndicator(
    session: OpenLibrary,
    state: PullToRefreshState,
    refreshing: Boolean,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val line by session.syncStatusStore.line.collectAsState()
    Column(modifier = modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        PullToRefreshDefaults.Indicator(state = state, isRefreshing = refreshing)
        val text = syncLineText(context, line)
        if (refreshing && text != null) {
            Text(
                text = text,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** Stands in for rows as one viewport-sized [LazyColumn] item, so the pull still reaches [PullToRefreshBox]. */
@Composable
internal fun ListPlaceholder(content: @Composable () -> Unit) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            Box(
                modifier = Modifier.fillParentMaxSize().padding(ThemeSpace.page),
                contentAlignment = Alignment.Center,
            ) {
                content()
            }
        }
    }
}
