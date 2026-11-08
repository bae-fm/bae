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
import fm.bae.app.OpenLibrary
import fm.bae.app.data.WindowedBrowserPageStore
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.LoadFailure
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.settings.syncLineText
import kotlinx.coroutines.launch

/**
 * A library browse tab's body: pull-to-refresh over load failure, nothing
 * while the first read runs, empty message or rows, each inside a scrollable so the pull reaches
 * [PullToRefreshBox]. The pull retries sync, and its spinner stays, over the
 * sync status line that says what sync is doing, until the pass the retry ran
 * ends.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibraryPageContent(
    session: OpenLibrary,
    page: WindowedBrowserPageStore<*, *, *>,
    emptyMessage: String,
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
    val pageError = page.error
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
        when {
            pageError != null && page.rows.isEmpty() -> {
                ListPlaceholder {
                    LoadFailure(pageError.error, onRetry = pageError.onRetry)
                }
            }

            // The first window is a local read; nothing shows until it lands.
            page.loading && page.rows.isEmpty() -> {
                ListPlaceholder {}
            }

            page.totalCount == 0 -> {
                ListPlaceholder {
                    Text(
                        text = emptyMessage,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            else -> {
                rows()
            }
        }
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
private fun ListPlaceholder(content: @Composable () -> Unit) {
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
