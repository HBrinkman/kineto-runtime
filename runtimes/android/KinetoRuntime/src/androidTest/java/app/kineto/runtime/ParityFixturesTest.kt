package app.kineto.runtime

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Runs runtimes/parity/runtime_fixtures.json on the device, through JNI, with
 * the same expectations as runtimes/parity/run_wasm_parity.mjs.
 */
@RunWith(AndroidJUnit4::class)
class ParityFixturesTest {
    private val assets = InstrumentationRegistry.getInstrumentation().context.assets

    private fun bytes(path: String) = assets.open(path).use { it.readBytes() }

    @Test
    fun everyPortableFixtureMatchesTheOtherRuntimes() {
        val manifest = JSONObject(String(bytes("runtimes/parity/runtime_fixtures.json")))
        val fixtures = manifest.getJSONArray("fixtures")
        var executed = 0
        for (i in 0 until fixtures.length()) {
            val fixture = fixtures.getJSONObject(i)
            // A fixture with a platform belongs to that platform's own runner.
            if (fixture.has("platform")) continue
            run(fixture)
            executed++
        }
        assertTrue("no fixtures ran: the manifest was not read", executed >= 6)
    }

    private fun run(fixture: JSONObject) {
        val id = fixture.getString("id")
        val steps = fixture.getJSONArray("steps")
        var player: KinetoPlayer? = null
        try {
            for (i in 0 until steps.length()) {
                val step = steps.getJSONObject(i)
                if (step.getString("op") == "load") {
                    player = KinetoPlayer.load(bytes(fixture.getString("asset")))
                    continue
                }
                execute(id, fixture, assertNotNull("$id: not loaded", player).let { player!! }, step)
            }
        } finally {
            player?.close()
        }
    }

    private fun execute(id: String, fixture: JSONObject, player: KinetoPlayer, step: JSONObject) {
        when (val op = step.getString("op")) {
            "advance" -> player.advance(step.getDouble("deltaSeconds"))
            "pointerEvent" -> {
                val event = KinetoPlayer.PointerEvent.valueOf(step.getString("event").uppercase())
                val fired = player.pointerEvent(event, step.getDouble("x"), step.getDouble("y"))
                if (step.has("expectFired")) {
                    assertEquals("$id: listeners fired", step.getInt("expectFired"), fired)
                }
            }
            "setNumber" -> player.setNumber(
                step.getString("stateMachineId"),
                step.getString("inputName"),
                step.getDouble("value"),
            )
            "setTrigger" -> player.setTrigger(
                step.getString("stateMachineId"),
                step.getString("inputName"),
            )
            "expectActiveState" -> assertEquals(
                "$id: active state of ${step.getString("stateMachineId")}",
                step.getString("stateId"),
                player.activeState(step.getString("stateMachineId")),
            )
            "expectOverrideScalar" -> {
                val actual = player.overrideScalar(step.getString("nodeId"), step.getString("key"))
                assertNotNull("$id: no ${step.getString("nodeId")}.${step.getString("key")}", actual)
                assertEquals(
                    "$id: ${step.getString("nodeId")}.${step.getString("key")}",
                    step.getDouble("value"),
                    actual!!,
                    1e-6,
                )
            }
            "expectBackground" -> {
                val rgba = step.getJSONArray("rgba")
                assertEquals(
                    "$id: background",
                    List(rgba.length()) { rgba.getInt(it) },
                    player.backgroundColor().toList(),
                )
            }
            "hitTestDocumentPoint" -> {
                val expected = fixture.getJSONObject("expect").opt("nodeId")
                    ?.takeUnless { it == JSONObject.NULL } as String?
                assertEquals(
                    "$id: hit test",
                    expected,
                    player.hitTestDocumentPoint(step.getDouble("x"), step.getDouble("y")),
                )
            }
            else -> fail("$id: this runner does not know op $op")
        }
    }
}
