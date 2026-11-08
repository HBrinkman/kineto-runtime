package app.kineto.runtime

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The public player contract on a device. The sound expectations are the ones
 * engine/kineto_engine/tests/audio_events.rs pins on the host, so a difference
 * here is a difference in the Android path, not in the fixture.
 */
@RunWith(AndroidJUnit4::class)
class PlayerContractTest {
    private val assets = InstrumentationRegistry.getInstrumentation().context.assets

    private fun sounds() = KinetoPlayer.load(
        assets.open("engine/kineto_engine/tests/fixtures/sounds_demo.kvk").use { it.readBytes() },
    )

    @Test
    fun garbageIsALoadErrorNotACrash() {
        for (bytes in listOf(byteArrayOf(0, 1, 2), ByteArray(4096) { 0x7f })) {
            val error = assertThrows(KinetoRuntimeException::class.java) { KinetoPlayer.load(bytes) }
            assertEquals(KinetoStatus.LOAD_FAILED, error.status)
        }
        // No bytes at all is the C ABI's "no input" status, on every host.
        val empty = assertThrows(KinetoRuntimeException::class.java) { KinetoPlayer.load(ByteArray(0)) }
        assertEquals(KinetoStatus.NULL_POINTER, empty.status)
    }

    @Test
    fun aTruncatedDocumentIsALoadErrorNotACrash() {
        val whole = assets.open("engine/kineto_engine/tests/fixtures/sounds_demo.kvk")
            .use { it.readBytes() }
        val error = assertThrows(KinetoRuntimeException::class.java) {
            KinetoPlayer.load(whole.copyOf(whole.size / 2))
        }
        assertEquals(KinetoStatus.LOAD_FAILED, error.status)
    }

    @Test
    fun aClosedPlayerRefusesEveryCallAndClosesTwiceSafely() {
        val player = sounds()
        player.close()
        player.close()
        assertTrue(player.isClosed)
        assertThrows(KinetoRuntimeException::class.java) { player.advance(0.016) }
        assertThrows(KinetoRuntimeException::class.java) {
            player.setTrigger("machine-button", "press")
        }
        assertThrows(KinetoRuntimeException::class.java) {
            player.pointerEvent(KinetoPlayer.PointerEvent.DOWN, 0.0, 0.0)
        }
        assertThrows(KinetoRuntimeException::class.java) { player.takeFiredAudioEvents() }
        assertThrows(KinetoRuntimeException::class.java) { player.play() }
    }

    @Test
    fun anUnknownInputIsReportedWithTheEngineMessage() {
        sounds().use { player ->
            val error = assertThrows(KinetoRuntimeException::class.java) {
                player.setTrigger("machine-button", "no-such-input")
            }
            assertEquals(KinetoStatus.INVALID_ARGUMENT, error.status)
            assertFalse(error.message.isNullOrBlank())

            assertThrows(KinetoRuntimeException::class.java) {
                player.transitionProgress("no-such-machine")
            }
            assertThrows(KinetoRuntimeException::class.java) {
                player.selectAnimation("no-such-clip")
            }
            assertNull(player.activeState("no-such-machine"))
        }
    }

    @Test
    fun aDocumentDescribesItself() {
        sounds().use { player ->
            assertEquals(listOf("machine-button"), player.stateMachines().map { it.id })
            val press = player.inputs("machine-button").single { it.id == "press" || it.name == "press" }
            assertEquals(KinetoPlayer.InputInfo.Kind.TRIGGER, press.kind)
            assertTrue(player.inputs("no-such-machine").isEmpty())
            assertTrue(player.clips().any { it.id == "clip-idle" && it.duration > 0.0 })
        }
    }

    @Test
    fun aTriggerReportsItsSoundsInOrder() {
        sounds().use { player ->
            assertEquals(
                listOf("event-click" to "ClickSound", "event-success" to "Success"),
                player.audioEvents().map { it.id to it.name },
            )
            assertEquals("click.wav", player.audioEvents().first().sourceName)

            player.setTrigger("machine-button", "press")
            player.advance(0.016)
            assertEquals(listOf("event-click"), player.takeFiredAudioEvents())
            // Draining: the same firing is never reported twice.
            assertTrue(player.takeFiredAudioEvents().isEmpty())

            player.advance(1.1)
            assertEquals(listOf("event-success"), player.takeFiredAudioEvents())
        }
    }

    @Test
    fun aTapReportsTheListenerAndItsSound() {
        sounds().use { player ->
            val fired = player.pointerEvent(KinetoPlayer.PointerEvent.DOWN, 400.0, 300.0)
            assertEquals(1, fired)
            assertEquals(listOf("event-click"), player.takeFiredAudioEvents())
        }
    }

    @Test
    fun theTimelineAudioFollowsThePlayhead() {
        sounds().use { player ->
            assertTrue(player.activeAudioClips().isEmpty())

            player.selectAnimation("clip-idle")
            player.play()
            assertTrue(player.activeAudioClips().isEmpty())

            player.advance(1.5)
            val clip = player.activeAudioClips().single()
            assertEquals("track-1", clip.trackId)
            assertEquals("success.wav", clip.fileName)
            assertEquals(0.5, clip.sourcePosition, 1e-3)
            assertEquals(0.8, clip.volume, 1e-3)

            player.advance(1.0)
            assertTrue(player.activeAudioClips().isEmpty())

            assertTrue(player.audioAssets().any { it.fileName == "success.wav" })
        }
    }

    @Test
    fun aSurfacePointMapsThroughTheEngineFit() {
        sounds().use { player ->
            // The same document point from two surfaces: the mapping has to
            // depend on the surface, or a host's touches land off the artwork.
            val small = player.documentPoint(400.0, 300.0, 200.0, 150.0)
            val large = player.documentPoint(1600.0, 1200.0, 800.0, 600.0)
            assertEquals(small.x, large.x, 1e-6)
            assertEquals(small.y, large.y, 1e-6)
            // And it lands on the artwork: the middle of the surface is the
            // middle of the tappable shape in this document.
            assertEquals(1, player.pointerEvent(KinetoPlayer.PointerEvent.DOWN, small.x, small.y))
        }
    }
}
