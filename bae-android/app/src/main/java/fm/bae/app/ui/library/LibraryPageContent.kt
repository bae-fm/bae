package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.WindowedBrowserPageStore
import fm.bae.app.ui.appearance.ThemeSpace
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// Hold the spinner briefly past the sync trigger so it doesn't snap away before
// the refreshed rows land.
private const val PULL_REFRESH_SETTLE_MS = 900L

/**
 * A library browse tab's body: pull-to-refresh over load failure, spinner, empty
 * message or rows, each inside a scrollable so the pull reaches [PullToRefreshBox].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibraryPageContent(
    session: OpenLibrary,
    page: WindowedBrowserPageStore<*, *>,
    emptyMessage: String,
    rows: @Composable () -> Unit,
) {
    var refreshing by remember { mutableStateOf(false) }
    val refreshScope = rememberCoroutineScope()
    val onRefresh: () -> Unit = {
        session.appHandle.triggerSync()
        refreshScope.launch {
            refreshing = true
            delay(PULL_REFRESH_SETTLE_MS)
            refreshing = false
        }
    }
    val pageError = page.error
    PullToRefreshBox(isRefreshing = refreshing, onRefresh = onRefresh, modifier = Modifier.fillMaxSize()) {
        when {
            pageError != null && page.rows.isEmpty() -> {
                ListPlaceholder {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center,
                    ) {
                        Text(text = pageError.message, color = MaterialTheme.colorScheme.error)
                        TextButton(onClick = pageError.onRetry) { Text(stringResource(R.string.retry)) }
                    }
                }
            }

            page.loading && page.rows.isEmpty() -> {
                ListPlaceholder { CircularProgressIndicator() }
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
