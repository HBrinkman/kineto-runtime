package app.kineto.runtime

import android.content.Context
import android.view.Choreographer
import android.view.Gravity
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.widget.FrameLayout
import android.widget.TextView

/**
 * Plays a [KinetoPlayer] on a surface and delivers touch to it.
 *
 * Main thread only, like any view. The view does not own the player: closing it is
 * the host's job, and closing it while attached stops the frame loop and reports
 * through [lastError] rather than crashing.
 */
class KinetoPlayerView(context: Context) : FrameLayout(context), SurfaceHolder.Callback {
    var player: KinetoPlayer? = null
        set(value) {
            field = value
            pointerDown = false
            report(if (value == null) "Kineto runtime: no player loaded" else null)
            renderFrame()
            syncFrameLoop()
        }

    var autoplay: Boolean = true
        set(value) {
            field = value
            syncFrameLoop()
        }

    /** After each successful frame, for a host polling state or pumping audio. */
    var onFrame: (() -> Unit)? = null

    /** With the number of listeners a touch fired, 0 when it could not be delivered. */
    var onPointerEvent: ((KinetoPlayer.PointerEvent, Int) -> Unit)? = null

    /** What is stopping playback, or null while everything works. Also shown over the surface. */
    var lastError: String? = null
        private set

    private var surfaceHandle: Long = 0
    private var framePosted = false
    private var lastFrameNanos = 0L
    private var pointerDown = false

    private val frameCallback = Choreographer.FrameCallback { frameTimeNanos ->
        framePosted = false
        val currentPlayer = player
        if (!autoplay || currentPlayer == null || surfaceHandle == 0L) {
            lastFrameNanos = 0L
            return@FrameCallback
        }

        if (lastFrameNanos != 0L) {
            val deltaSeconds = ((frameTimeNanos - lastFrameNanos) / 1_000_000_000.0).coerceAtMost(0.05)
            try {
                currentPlayer.advance(deltaSeconds)
            } catch (error: KinetoRuntimeException) {
                // Reposting would hit the same error at 60 Hz. A new player or a
                // new surface starts the loop again.
                report(error.message ?: "Kineto advance failed")
                return@FrameCallback
            }
        }

        lastFrameNanos = frameTimeNanos
        if (renderFrame()) syncFrameLoop()
    }

    private val surfaceView = SurfaceView(context).apply {
        holder.addCallback(this@KinetoPlayerView)
    }

    private val statusView = TextView(context).apply {
        gravity = Gravity.CENTER
        text = "Kineto runtime: no player loaded"
    }

    init {
        addView(
            surfaceView,
            LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT),
        )
        addView(
            statusView,
            LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT),
        )
    }

    /** Draws the current frame. Returns whether it reached the surface. */
    fun renderFrame(): Boolean {
        val currentPlayer = player ?: return false
        val currentSurface = surfaceHandle
        if (currentSurface == 0L) return false

        return try {
            currentPlayer.render(currentSurface)
            report(null)
            onFrame?.invoke()
            true
        } catch (error: KinetoRuntimeException) {
            report(error.message ?: "Kineto render failed")
            false
        }
    }

    /**
     * Touch, in the document's own coordinates. The engine undoes the fit and camera
     * it renders with. View pixels are surface pixels here: the surface fills the view.
     */
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (player == null) return false
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                pointerDown = true
                deliver(KinetoPlayer.PointerEvent.DOWN, event)
            }
            // A drag drives, it does not fire: there is no move listener.
            MotionEvent.ACTION_MOVE -> if (pointerDown) moved(event)
            MotionEvent.ACTION_UP -> release(event)
            // A cancelled touch is an up to the machine: leaving a pointer-down
            // listener latched would strand the state.
            MotionEvent.ACTION_CANCEL -> release(event)
        }
        return true
    }

    private fun release(event: MotionEvent) {
        if (!pointerDown) return
        pointerDown = false
        deliver(KinetoPlayer.PointerEvent.UP, event)
    }

    private fun documentPoint(player: KinetoPlayer, event: MotionEvent) = player.documentPoint(
        width.toDouble(),
        height.toDouble(),
        event.x.toDouble(),
        event.y.toDouble(),
    )

    private fun deliver(kind: KinetoPlayer.PointerEvent, event: MotionEvent) {
        val currentPlayer = player ?: return
        val fired = try {
            val point = documentPoint(currentPlayer, event)
            currentPlayer.pointerEvent(kind, point.x, point.y)
        } catch (error: KinetoRuntimeException) {
            report(error.message ?: "Kineto pointer event failed")
            0
        }
        onPointerEvent?.invoke(kind, fired)
    }

    private fun moved(event: MotionEvent) {
        val currentPlayer = player ?: return
        try {
            val point = documentPoint(currentPlayer, event)
            currentPlayer.pointerMoved(point.x, point.y)
        } catch (error: KinetoRuntimeException) {
            report(error.message ?: "Kineto pointer move failed")
        }
    }

    private fun report(message: String?) {
        lastError = message
        statusView.text = message.orEmpty()
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        attachSurface(holder)
    }

    override fun surfaceChanged(
        holder: SurfaceHolder,
        format: Int,
        width: Int,
        height: Int,
    ) {
        if (surfaceHandle == 0L) {
            attachSurface(holder)
            return
        }
        if (width <= 0 || height <= 0) return
        val status = KinetoRuntimeJni.nativeResizeSurface(surfaceHandle, width, height)
        if (status == STATUS_OK) {
            renderFrame()
        } else {
            report(surfaceError(status))
        }
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        destroySurface()
    }

    override fun onDetachedFromWindow() {
        stopFrameLoop()
        super.onDetachedFromWindow()
    }

    private fun attachSurface(holder: SurfaceHolder) {
        destroySurface()
        val width = surfaceView.width.takeIf { it > 0 } ?: holder.surfaceFrame.width()
        val height = surfaceView.height.takeIf { it > 0 } ?: holder.surfaceFrame.height()
        if (width <= 0 || height <= 0) {
            report("Kineto runtime: surface has no size")
            return
        }

        surfaceHandle = KinetoRuntimeJni.nativeAttachSurface(holder.surface, width, height)
        if (surfaceHandle == 0L) {
            report(surfaceError(STATUS_RENDER_FAILED))
            return
        }
        renderFrame()
        syncFrameLoop()
    }

    private fun destroySurface() {
        stopFrameLoop()
        val handle = surfaceHandle
        if (handle == 0L) return
        surfaceHandle = 0
        KinetoRuntimeJni.nativeDestroySurface(handle)
    }

    private fun syncFrameLoop() {
        if (!autoplay || player == null || surfaceHandle == 0L) {
            stopFrameLoop()
            return
        }
        if (framePosted) return
        framePosted = true
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    private fun stopFrameLoop() {
        if (framePosted) {
            Choreographer.getInstance().removeFrameCallback(frameCallback)
            framePosted = false
        }
        lastFrameNanos = 0L
    }

    private fun surfaceError(status: Int): String {
        val message = KinetoRuntimeJni.nativeSurfaceLastError()
        return message.ifBlank { "Kineto surface operation failed ($status)" }
    }

    companion object {
        private const val STATUS_OK = 0
        private const val STATUS_RENDER_FAILED = 5
    }
}
