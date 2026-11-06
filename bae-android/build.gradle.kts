plugins {
    id("com.android.application") version "8.7.3" apply false
    id("com.android.library") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.0.21" apply false
    // Renders @Preview composables to PNGs on the JVM (layoutlib, no emulator);
    // scripts/shots/android.sh drives it to capture the screenshot scenes.
    // alpha09 is the line that pairs with AGP 8.7.x (later alphas require AGP
    // 8.13+ and the @PreviewTest opt-in annotation).
    id("com.android.compose.screenshot") version "0.0.1-alpha09" apply false
}

// A JVM unit test that hangs fails the run within testTimeoutSeconds and names itself.
// The limit sits above the tests' HANG_GUARD (300 s), so a test stuck on a bounded wait
// fails with that wait's message; this is for hangs nothing bounds.
// Gradle's Test task times out only as a whole, and JUnit 4 has no global per-test
// timeout, so the build watches each test from outside: once one has run past the
// limit, the watchdog logs its name, asks the test JVM for a thread dump, and kills
// the JVM, which Gradle reports as a failed test run.
val testTimeoutSeconds = 360L

subprojects {
    tasks.withType<Test>().configureEach {
        val started = java.util.concurrent.ConcurrentHashMap<String, Long>()
        val watchdog = java.util.concurrent.Executors.newSingleThreadScheduledExecutor { r ->
            Thread(r, "test-timeout-watchdog").apply { isDaemon = true }
        }
        addTestListener(
            object : TestListener {
                override fun beforeSuite(suite: TestDescriptor) = Unit
                override fun afterSuite(suite: TestDescriptor, result: TestResult) = Unit
                override fun beforeTest(testDescriptor: TestDescriptor) {
                    started["${testDescriptor.className}.${testDescriptor.name}"] = System.nanoTime()
                }
                override fun afterTest(testDescriptor: TestDescriptor, result: TestResult) {
                    started.remove("${testDescriptor.className}.${testDescriptor.name}")
                }
            },
        )
        doFirst {
            watchdog.scheduleWithFixedDelay(
                {
                    val limit = java.util.concurrent.TimeUnit.SECONDS.toNanos(testTimeoutSeconds)
                    val hung = started.entries.firstOrNull { System.nanoTime() - it.value > limit }
                    if (hung != null) {
                        logger.error("TEST TIMEOUT: ${hung.key} ran longer than ${testTimeoutSeconds}s; killing the test JVM")
                        val workers = ProcessHandle.current().descendants()
                            .filter { it.info().commandLine().orElse("").contains("Gradle Test Executor") }
                            .toList()
                        workers.forEach { ProcessBuilder("kill", "-QUIT", it.pid().toString()).start().waitFor() }
                        Thread.sleep(3_000)
                        workers.forEach { it.destroyForcibly() }
                        started.clear()
                    }
                },
                5, 5, java.util.concurrent.TimeUnit.SECONDS,
            )
        }
        doLast { watchdog.shutdownNow() }
    }
}
