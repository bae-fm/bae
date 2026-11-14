package fm.bae.app.ui

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.R
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

/** Finishing a removal is retried only where asking again can get past the failure. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class RemovalIncompleteScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aFailureThatMayPassOffersRetry() {
        compose.setContent {
            TestTheme { RemovalIncompleteScreen(error = DisplayError("Offline.", "offline"), onRetry = {}) }
        }

        compose.onNodeWithText(compose.activity.getString(R.string.retry)).assertIsDisplayed()
    }

    @Test
    fun aFailureRetryCannotPassOffersNoRetry() {
        val damaged = DisplayError("Something went wrong with the database.", "damaged", offersRetry = false, resetFromCloudReplaces = true)
        compose.setContent {
            TestTheme { RemovalIncompleteScreen(error = damaged, onRetry = {}) }
        }

        compose.onAllNodesWithText(compose.activity.getString(R.string.retry)).assertCountEquals(0)
        compose.onAllNodesWithText(compose.activity.getString(R.string.reset_device)).assertCountEquals(0)
    }
}
