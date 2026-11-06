package fm.bae.app

import org.junit.runners.model.FrameworkMethod
import org.junit.runners.model.Statement
import org.robolectric.RobolectricTestRunner
import org.robolectric.internal.SandboxTestRunner
import java.time.Duration

/**
 * The runner of every Robolectric test: frames are paced by the looper's
 * clock, as a display paces them, from the moment the test method starts.
 * Unpaced, Robolectric runs a frame as soon as one is asked for, at the same
 * instant, so a view that asks for the next frame from each one (an animation,
 * a progress bar following a position) never lets the looper go idle, and the
 * test's idle wait spins forever.
 *
 * Pacing starts at the test method, after the test's rules and `@Before`
 * methods, because a paced frame waits out its delay: launching an activity
 * through `ActivityScenario` (every compose rule) needs its first frame to run
 * at once, and without it the launch finds no window.
 *
 * A test with a compose rule is not paced: the rule drives its own clock, and
 * with the Choreographer paced its idle wait never finishes
 * (`ClosingScreenTest` hung for the 360 s watchdog).
 */
class BaeRobolectricTestRunner(
    testClass: Class<*>,
) : RobolectricTestRunner(testClass) {
    override fun getHelperTestRunner(bootstrappedTestClass: Class<*>): SandboxTestRunner.HelperTestRunner =
        PacedHelperTestRunner(bootstrappedTestClass)

    private class PacedHelperTestRunner(
        testClass: Class<*>,
    ) : HelperTestRunner(testClass) {
        override fun methodInvoker(
            method: FrameworkMethod,
            test: Any,
        ): Statement {
            val invoke = super.methodInvoker(method, test)
            return object : Statement() {
                override fun evaluate() {
                    if (!hasComposeRule(test.javaClass)) paceFrames(test.javaClass.classLoader)
                    invoke.evaluate()
                }
            }
        }
    }

    private companion object {
        val FRAME_INTERVAL: Duration = Duration.ofMillis(16)

        fun hasComposeRule(testClass: Class<*>): Boolean =
            generateSequence(testClass) { it.superclass }
                .flatMap { it.declaredFields.asSequence() }
                .any { isComposeRule(it.type) }

        fun isComposeRule(type: Class<*>): Boolean =
            type.name == "androidx.compose.ui.test.junit4.ComposeTestRule" ||
                (listOfNotNull(type.superclass) + type.interfaces).any(::isComposeRule)

        /**
         * Robolectric's frame state is static in the class loader of the test's
         * sandbox, which is not this runner's, so it is set through the sandbox's copy.
         */
        fun paceFrames(sandbox: ClassLoader) {
            val choreographer = sandbox.loadClass("org.robolectric.shadows.ShadowChoreographer")
            choreographer.getMethod("setPaused", Boolean::class.javaPrimitiveType).invoke(null, true)
            choreographer.getMethod("setFrameDelay", Duration::class.java).invoke(null, FRAME_INTERVAL)
        }
    }
}
