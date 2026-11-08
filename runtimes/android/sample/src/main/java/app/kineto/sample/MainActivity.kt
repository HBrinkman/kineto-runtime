package app.kineto.sample

import android.app.Activity
import android.os.Bundle
import android.widget.LinearLayout
import android.widget.TextView
import app.kineto.runtime.KinetoPlayer
import app.kineto.runtime.KinetoPlayerView
import app.kineto.runtime.KinetoRuntimeException
import java.io.IOException

/**
 * An independent host: a plain Android app that depends on the runtime's public
 * API and an exported `.kvk`, nothing else. If something is only reachable from
 * inside the runtime module, this file will not compile.
 */
class MainActivity : Activity() {
    lateinit var playerView: KinetoPlayerView
        private set
    lateinit var status: TextView
        private set
    var player: KinetoPlayer? = null
        private set

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        status = TextView(this)
        playerView = KinetoPlayerView(this)
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                fitsSystemWindows = true
                addView(status, LinearLayout.LayoutParams(MATCH, WRAP))
                addView(playerView, LinearLayout.LayoutParams(MATCH, 0, 1f))
            },
        )
        load(intent.getStringExtra(EXTRA_ASSET) ?: DEFAULT_ASSET)
    }

    private fun load(asset: String) {
        try {
            player = KinetoPlayer.loadAsset(this, asset)
            playerView.player = player
            status.text = asset
        } catch (error: KinetoRuntimeException) {
            status.text = "Unable to load $asset: ${error.message}"
        } catch (error: IOException) {
            status.text = "Unable to read $asset: ${error.message}"
        }
    }

    override fun onDestroy() {
        playerView.player = null
        player?.close()
        player = null
        super.onDestroy()
    }

    companion object {
        const val EXTRA_ASSET = "asset"
        const val DEFAULT_ASSET = "sounds_demo.kvk"
        private const val MATCH = LinearLayout.LayoutParams.MATCH_PARENT
        private const val WRAP = LinearLayout.LayoutParams.WRAP_CONTENT
    }
}
