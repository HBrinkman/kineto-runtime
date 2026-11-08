package app.kineto.sample

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.view.MotionEvent
import android.view.PixelCopy
import android.view.SurfaceView
import android.view.ViewGroup
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.kineto.runtime.KinetoPlayer
import app.kineto.runtime.KinetoPlayerView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * A plain Android app playing an exported `.kvk`: real surface, real pixels read
 * back from the compositor, real touch events through the view tree.
 *
 * On the emulator the GPU is lavapipe. These prove the path, not the look on
 * hardware; docs/PLATFORM_SUPPORT.md keeps the two apart.
 */
@RunWith(AndroidJUnit4::class)
class HostSmokeTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private fun launch(asset: String = MainActivity.DEFAULT_ASSET): ActivityScenario<MainActivity> {
        val intent = Intent(ApplicationProvider.getApplicationContext(), MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_ASSET, asset)
        return ActivityScenario.launch(intent)
    }

    private fun ActivityScenario<MainActivity>.playerView(): KinetoPlayerView {
        lateinit var view: KinetoPlayerView
        onActivity { view = it.playerView }
        return view
    }

    private fun KinetoPlayerView.surfaceView(): SurfaceView =
        (0 until childCount).map(::getChildAt).filterIsInstance<SurfaceView>().single()

    /** What the compositor shows for the surface, or null while it has no buffer yet. */
    private fun capture(view: KinetoPlayerView): Bitmap? {
        val surface = view.surfaceView()
        if (surface.width == 0 || surface.height == 0 || !surface.holder.surface.isValid) return null
        val bitmap = Bitmap.createBitmap(surface.width, surface.height, Bitmap.Config.ARGB_8888)
        val thread = HandlerThread("pixel-copy").apply { start() }
        try {
            val done = CountDownLatch(1)
            var result = PixelCopy.ERROR_UNKNOWN
            PixelCopy.request(surface, bitmap, {
                result = it
                done.countDown()
            }, Handler(thread.looper))
            done.await(5, TimeUnit.SECONDS)
            return bitmap.takeIf { result == PixelCopy.SUCCESS }
        } finally {
            thread.quitSafely()
        }
    }

    private fun Bitmap.distinctColors(): Int {
        val seen = HashSet<Int>()
        for (y in 0 until height step 8) {
            for (x in 0 until width step 8) seen += getPixel(x, y)
        }
        return seen.size
    }

    private fun waitForContent(view: KinetoPlayerView, timeoutMs: Long = SURFACE_TIMEOUT_MS): Bitmap {
        val deadline = SystemClock.uptimeMillis() + timeoutMs
        var last: Bitmap? = null
        while (SystemClock.uptimeMillis() < deadline) {
            last = capture(view) ?: last
            if (last != null && last.distinctColors() > 1) return last
            SystemClock.sleep(100)
        }
        throw AssertionError("no drawn content within ${timeoutMs}ms; last=${last?.distinctColors()} colours")
    }

    private fun tap(view: KinetoPlayerView, x: Float, y: Float) {
        val down = SystemClock.uptimeMillis()
        val location = IntArray(2)
        instrumentation.runOnMainSync { view.getLocationInWindow(location) }
        for ((action, time) in listOf(MotionEvent.ACTION_DOWN to down, MotionEvent.ACTION_UP to down + 50)) {
            val event = MotionEvent.obtain(down, time, action, x + location[0], y + location[1], 0)
            instrumentation.runOnMainSync { view.rootView.dispatchTouchEvent(event) }
            event.recycle()
        }
    }

    @Test
    fun anExportedDocumentReachesTheScreen() {
        launch().use { scenario ->
            val frame = waitForContent(scenario.playerView())
            // The fixture's tappable shape covers the middle of the document, and
            // the document is fitted to the surface. Middle is artwork, corner is not.
            val middle = frame.getPixel(frame.width / 2, frame.height / 2)
            val corner = frame.getPixel(4, 4)
            assertTrue("middle ${Color.valueOf(middle)} equals corner", middle != corner)
        }
    }

    @Test
    fun aRunningAnimationChangesThePixels() {
        // animated_sample.kvk loops opacity and positionX on one box.
        launch("animated_sample.kvk").use { scenario ->
            val view = scenario.playerView()
            assertTrue("the frame never changed in 5s", frameChanges(view, waitForContent(view)))
        }
    }

    @Test
    fun aDocumentWithNothingMovingKeepsItsPixels() {
        // The control for the test above: without it, a frame "changing" could
        // be capture noise rather than animation.
        launch("rig_parity.kvk").use { scenario ->
            val view = scenario.playerView()
            SystemClock.sleep(3_000)
            val first = capture(view) ?: throw AssertionError("no frame")
            assertFalse("a static document changed pixels", frameChanges(view, first, 1_500))
        }
    }

    private fun frameChanges(view: KinetoPlayerView, first: Bitmap, withinMs: Long = 5_000): Boolean {
        val deadline = SystemClock.uptimeMillis() + withinMs
        while (SystemClock.uptimeMillis() < deadline) {
            val next = capture(view)
            if (next != null && !next.sameAs(first)) return true
            SystemClock.sleep(100)
        }
        return false
    }

    @Test
    fun aTapOnTheArtworkFiresItsListener() {
        launch().use { scenario ->
            val view = scenario.playerView()
            waitForContent(view)
            // The player runs on the main thread's frame loop, so drain what it
            // has already reported before the tap we are measuring.
            lateinit var player: KinetoPlayer
            scenario.onActivity { player = it.player!! }
            player.takeFiredAudioEvents()

            tap(view, view.width / 2f, view.height / 2f)

            assertEquals(listOf("event-click"), player.takeFiredAudioEvents())
        }
    }

    @Test
    fun aTapBesideTheArtworkFiresNothing() {
        launch().use { scenario ->
            val view = scenario.playerView()
            waitForContent(view)
            lateinit var player: KinetoPlayer
            scenario.onActivity { player = it.player!! }
            player.takeFiredAudioEvents()

            tap(view, 4f, 4f)

            assertTrue(player.takeFiredAudioEvents().isEmpty())
        }
    }

    @Test
    fun theSurfaceComesBackAfterTheAppIsBackgrounded() {
        launch().use { scenario ->
            val view = scenario.playerView()
            waitForContent(view)

            scenario.moveToState(androidx.lifecycle.Lifecycle.State.CREATED)
            SystemClock.sleep(500)
            scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED)

            waitForContent(scenario.playerView())
        }
    }

    @Test
    fun recreatingTheActivityLeavesAWorkingPlayer() {
        launch().use { scenario ->
            waitForContent(scenario.playerView())
            scenario.recreate()
            waitForContent(scenario.playerView())
            lateinit var name: String
            scenario.onActivity { name = it.player!!.stateMachines().single().id }
            assertEquals("machine-button", name)
        }
    }

    @Test
    fun aResizeRedrawsAtTheNewSize() {
        launch().use { scenario ->
            val view = scenario.playerView()
            val before = waitForContent(view)
            instrumentation.runOnMainSync {
                view.layoutParams = (view.layoutParams as ViewGroup.LayoutParams).apply {
                    height = view.height / 2
                }
                (view.layoutParams as android.widget.LinearLayout.LayoutParams).weight = 0f
                view.requestLayout()
            }
            val deadline = SystemClock.uptimeMillis() + 5_000
            var after: Bitmap? = null
            while (SystemClock.uptimeMillis() < deadline) {
                after = capture(view)
                if (after != null && after.height < before.height && after.distinctColors() > 1) break
                SystemClock.sleep(100)
            }
            assertTrue("did not redraw smaller: ${after?.height} vs ${before.height}", after!!.height < before.height)
            assertTrue(after.distinctColors() > 1)
        }
    }

    @Test
    fun closingThePlayerUnderAViewStopsDrawingWithoutACrash() {
        launch().use { scenario ->
            val view = scenario.playerView()
            waitForContent(view)
            scenario.onActivity { it.player!!.close() }
            SystemClock.sleep(500)
            // Touch on a closed player must not throw out of the view tree.
            tap(view, view.width / 2f, view.height / 2f)
            SystemClock.sleep(200)
            scenario.onActivity { assertTrue(it.player!!.isClosed) }
        }
    }

    @Test
    fun theViewReportsTheListenerAndKeepsTheFrameCallbackGoing() {
        launch().use { scenario ->
            val view = scenario.playerView()
            waitForContent(view)
            val fired = mutableListOf<Pair<KinetoPlayer.PointerEvent, Int>>()
            val frames = java.util.concurrent.atomic.AtomicInteger()
            instrumentation.runOnMainSync {
                view.onPointerEvent = { event, count -> fired += event to count }
                view.onFrame = { frames.incrementAndGet() }
            }
            tap(view, view.width / 2f, view.height / 2f)
            SystemClock.sleep(300)
            instrumentation.runOnMainSync {
                assertEquals(KinetoPlayer.PointerEvent.DOWN to 1, fired.first())
                assertEquals(KinetoPlayer.PointerEvent.UP, fired[1].first)
            }
            assertTrue("no frames reported", frames.get() > 0)
        }
    }

    @Test
    fun aCancelledTouchReleasesThePointer() {
        launch().use { scenario ->
            val view = scenario.playerView()
            waitForContent(view)
            val fired = mutableListOf<KinetoPlayer.PointerEvent>()
            instrumentation.runOnMainSync { view.onPointerEvent = { event, _ -> fired += event } }
            val now = SystemClock.uptimeMillis()
            val x = view.width / 2f
            val y = view.height / 2f
            for (action in listOf(MotionEvent.ACTION_DOWN, MotionEvent.ACTION_CANCEL)) {
                val event = MotionEvent.obtain(now, now, action, x, y, 0)
                instrumentation.runOnMainSync { view.dispatchTouchEvent(event) }
                event.recycle()
            }
            instrumentation.runOnMainSync {
                // Leaving a pointer-down latched would strand the state.
                assertEquals(
                    listOf(KinetoPlayer.PointerEvent.DOWN, KinetoPlayer.PointerEvent.UP),
                    fired,
                )
            }
        }
    }

    @Test
    fun aClosedPlayerIsReportedAndStopsTheFrameLoop() {
        launch().use { scenario ->
            val view = scenario.playerView()
            waitForContent(view)
            val frames = java.util.concurrent.atomic.AtomicInteger()
            scenario.onActivity {
                view.onFrame = { frames.incrementAndGet() }
                it.player!!.close()
            }
            SystemClock.sleep(300)
            val atClose = frames.get()
            SystemClock.sleep(300)
            assertEquals("frames kept coming after close", atClose, frames.get())
            instrumentation.runOnMainSync {
                assertEquals("KinetoPlayer is closed", view.lastError)
            }
        }
    }

    @Test
    fun aDocumentThatIsNotOneSaysSoInTheHost() {
        launch("does-not-exist.kvk").use { scenario ->
            scenario.onActivity {
                assertTrue(it.status.text.toString(), it.status.text.startsWith("Unable to"))
                assertEquals(null, it.player)
                assertFalse(it.isFinishing)
            }
        }
    }

    private companion object {
        // Pipeline creation is slow on software Vulkan.
        const val SURFACE_TIMEOUT_MS = 30_000L
    }
}
