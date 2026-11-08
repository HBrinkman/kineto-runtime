package app.kineto.runtime

import android.content.Context
import java.io.Closeable

/**
 * One loaded `.kvk`.
 *
 * Safe to call from any thread: every call that reaches the native handle takes
 * this object's lock, so a host calling [setTrigger] off the main thread cannot
 * race the frame a [KinetoPlayerView] is rendering, and [close] cannot free the
 * handle underneath one.
 */
class KinetoPlayer private constructor(private var nativeHandle: Long) : Closeable {
    @get:Synchronized
    val isClosed: Boolean
        get() = nativeHandle == 0L

    @Synchronized
    fun advance(deltaSeconds: Double) {
        checkStatus(KinetoRuntimeJni.nativeAdvance(openHandle(), deltaSeconds))
    }

    @Synchronized
    fun setBool(stateMachineId: String, inputName: String, value: Boolean) {
        checkStatus(KinetoRuntimeJni.nativeSetBool(openHandle(), stateMachineId, inputName, value))
    }

    @Synchronized
    fun setNumber(stateMachineId: String, inputName: String, value: Double) {
        checkStatus(KinetoRuntimeJni.nativeSetNumber(openHandle(), stateMachineId, inputName, value))
    }

    /**
     * A pointer event at a document point; whatever listens there drives its
     * input (issue #74). Returns how many listeners fired.
     */
    @Synchronized
    fun pointerEvent(event: PointerEvent, x: Double, y: Double): Int {
        val result = KinetoRuntimeJni.nativePointerEvent(openHandle(), event.ordinal, x, y)
        // Negative is a failed status, so a fired count and a status share the
        // one return value without a second JNI call.
        if (result < 0) checkStatus(-result)
        return result
    }

    /** A drag rather than a tap: fires no listeners, drives what follows the pointer. */
    @Synchronized
    fun pointerMoved(x: Double, y: Double) {
        checkStatus(KinetoRuntimeJni.nativePointerMoved(openHandle(), x, y))
    }

    /**
     * A point on a surface of [surfaceWidth] by [surfaceHeight] pixels, in document
     * space. The engine undoes the fit and camera it renders with, so a host never
     * repeats that arithmetic.
     */
    @Synchronized
    fun documentPoint(surfaceWidth: Double, surfaceHeight: Double, x: Double, y: Double): DocumentPoint {
        val out = DoubleArray(2)
        checkStatus(
            KinetoRuntimeJni.nativeDocumentPoint(openHandle(), surfaceWidth, surfaceHeight, x, y, out),
        )
        return DocumentPoint(out[0], out[1])
    }

    /**
     * The document's background as straight `[r, g, b, a]`, 0-255. All zero when
     * it has none: the surface stays transparent.
     */
    @Synchronized
    fun backgroundColor(): IntArray {
        val out = IntArray(4)
        checkStatus(KinetoRuntimeJni.nativeBackgroundColor(openHandle(), out))
        return out
    }

    @Synchronized
    fun setTrigger(stateMachineId: String, inputName: String) {
        checkStatus(KinetoRuntimeJni.nativeSetTrigger(openHandle(), stateMachineId, inputName))
    }

    @Synchronized
    fun activeState(stateMachineId: String): String? =
        KinetoRuntimeJni.nativeActiveState(openHandle(), stateMachineId)

    @Synchronized
    fun activeTransition(stateMachineId: String): String? =
        KinetoRuntimeJni.nativeActiveTransition(openHandle(), stateMachineId)

    @Synchronized
    fun transitionProgress(stateMachineId: String): Double {
        val status = IntArray(1)
        val progress = KinetoRuntimeJni.nativeTransitionProgress(openHandle(), stateMachineId, status)
        checkStatus(status[0])
        return progress
    }

    @Synchronized
    fun hitTestDocumentPoint(x: Double, y: Double): String? =
        KinetoRuntimeJni.nativeHitTestDocumentPoint(openHandle(), x, y)

    /**
     * One evaluated scalar of the current frame, or null when the frame does not
     * carry it. For cross-runtime parity checks; hosts consume a frame by rendering.
     */
    @Synchronized
    fun overrideScalar(nodeId: String, key: String): Double? {
        val out = DoubleArray(1)
        val status = KinetoRuntimeJni.nativeOverrideScalar(openHandle(), nodeId, key, out)
        return if (status == STATUS_OK) out[0] else null
    }

    /** Starts the active clip. The timeline, and its audio, only move while playing. */
    @Synchronized
    fun play() {
        checkStatus(KinetoRuntimeJni.nativePlay(openHandle()))
    }

    /** Pauses playback, leaving the playhead where it is. */
    @Synchronized
    fun pause() {
        checkStatus(KinetoRuntimeJni.nativePause(openHandle()))
    }

    /** Makes [animationId] the active clip, rewound to its start. */
    @Synchronized
    fun selectAnimation(animationId: String) {
        checkStatus(KinetoRuntimeJni.nativeSelectAnimation(openHandle(), animationId))
    }

    @Synchronized
    fun stateMachines(): List<MachineInfo> =
        records(KinetoRuntimeJni.nativeStateMachines(openHandle()), 2).map {
            MachineInfo(id = it[0], name = it[1])
        }

    /** One machine's inputs. Empty when nothing carries [machineId]. */
    @Synchronized
    fun inputs(machineId: String): List<InputInfo> =
        records(KinetoRuntimeJni.nativeMachineInputs(openHandle(), machineId), 5).mapNotNull {
            // A kind this build does not know is skipped rather than guessed at:
            // driving an input by the wrong rule is worse than not offering it.
            val kind = it[2].toIntOrNull()?.let(InputInfo.Kind.entries::getOrNull)
                ?: return@mapNotNull null
            InputInfo(
                id = it[0],
                name = it[1],
                kind = kind,
                defaultBool = it[3] == "1",
                defaultNumber = it[4].toDoubleOrNull() ?: 0.0,
            )
        }

    @Synchronized
    fun clips(): List<ClipInfo> =
        records(KinetoRuntimeJni.nativeAnimations(openHandle()), 3).map {
            ClipInfo(id = it[0], name = it[1], duration = it[2].toDoubleOrNull() ?: 0.0)
        }

    /** Every audio file this document needs. A `.kvk` names its audio and carries none of it. */
    @Synchronized
    fun audioAssets(): List<AudioAssetInfo> =
        records(KinetoRuntimeJni.nativeAudioAssets(openHandle()), 3).map {
            AudioAssetInfo(id = it[0], fileName = it[1], duration = it[2].toDoubleOrNull() ?: 0.0)
        }

    /** The timeline audio that should be sounding at the playhead. Poll after [advance]. */
    @Synchronized
    fun activeAudioClips(): List<ActiveAudioClip> =
        records(KinetoRuntimeJni.nativeActiveAudioClips(openHandle()), 5).mapNotNull {
            val position = it[3].toDoubleOrNull() ?: return@mapNotNull null
            val volume = it[4].toDoubleOrNull() ?: return@mapNotNull null
            ActiveAudioClip(
                trackId = it[0],
                assetId = it[1],
                fileName = it[2],
                sourcePosition = position,
                volume = volume,
            )
        }

    /**
     * The sounds fired since the last call, in order. Call after [advance] and after
     * [pointerEvent]. Draining and never de-duplicated: the same sound twice is two
     * firings.
     */
    @Synchronized
    fun takeFiredAudioEvents(): List<String> =
        KinetoRuntimeJni.nativeTakeFiredAudioEvents(openHandle())
            .split('\n')
            .filter { it.isNotEmpty() }

    /** Every sound the document names, so a firing can be mapped to the app's own audio. */
    @Synchronized
    fun audioEvents(): List<AudioEvent> =
        records(KinetoRuntimeJni.nativeAudioEvents(openHandle()), 3).map {
            AudioEvent(id = it[0], name = it[1], sourceName = it[2].ifEmpty { null })
        }

    @Synchronized
    internal fun render(surfaceHandle: Long) {
        checkStatus(KinetoRuntimeJni.nativeRender(openHandle(), surfaceHandle))
    }

    @Synchronized
    fun lastError(): String {
        if (nativeHandle == 0L) return CLOSED_MESSAGE
        return KinetoRuntimeJni.nativeLastError(nativeHandle)
    }

    @Synchronized
    override fun close() {
        val handle = nativeHandle
        if (handle == 0L) return
        nativeHandle = 0L
        KinetoRuntimeJni.nativeDestroy(handle)
    }

    private fun openHandle(): Long {
        if (nativeHandle == 0L) {
            throw KinetoRuntimeException(KinetoStatus.NULL_POINTER, CLOSED_MESSAGE)
        }
        return nativeHandle
    }

    private fun checkStatus(code: Int) {
        if (code == STATUS_OK) return
        val message = lastError().ifBlank { "Kineto runtime operation failed ($code)" }
        throw KinetoRuntimeException(KinetoStatus.fromCode(code), message)
    }

    data class DocumentPoint(val x: Double, val y: Double)

    enum class PointerEvent { DOWN, UP, ENTER, EXIT }

    data class MachineInfo(val id: String, val name: String)

    data class InputInfo(
        val id: String,
        val name: String,
        val kind: Kind,
        val defaultBool: Boolean,
        val defaultNumber: Double,
    ) {
        /** The ordinal is the wire value. */
        enum class Kind { BOOL, TRIGGER, NUMBER }
    }

    data class ClipInfo(val id: String, val name: String, val duration: Double)

    data class AudioAssetInfo(val id: String, val fileName: String, val duration: Double)

    data class ActiveAudioClip(
        val trackId: String,
        val assetId: String,
        val fileName: String,
        val sourcePosition: Double,
        val volume: Double,
    )

    data class AudioEvent(val id: String, val name: String, val sourceName: String?)

    companion object {
        private const val STATUS_OK = 0
        private const val CLOSED_MESSAGE = "KinetoPlayer is closed"

        /** @throws KinetoRuntimeException with [KinetoStatus.LOAD_FAILED] when [bytes] is not a `.kvk`. */
        fun load(bytes: ByteArray): KinetoPlayer {
            val status = IntArray(1)
            val handle = KinetoRuntimeJni.nativeLoad(bytes, status)
            if (handle == 0L) {
                val resolved = KinetoStatus.fromCode(status[0]).takeUnless { it == KinetoStatus.OK }
                    ?: KinetoStatus.LOAD_FAILED
                throw KinetoRuntimeException(resolved, "Unable to load .kvk data")
            }
            return KinetoPlayer(handle)
        }

        /**
         * Takes over a reference the caller already holds. [close] gives it up. The
         * Flutter plugin uses this for the reference Dart retained for its view.
         */
        internal fun adopt(handle: Long): KinetoPlayer = KinetoPlayer(handle)

        fun loadAsset(context: Context, assetPath: String): KinetoPlayer {
            val bytes = context.assets.open(assetPath).use { it.readBytes() }
            return load(bytes)
        }

        /**
         * The runtime's newline/tab record format, dropping any row without the
         * expected field count: a malformed row is a bug to notice elsewhere, not
         * something to half-render.
         */
        private fun records(joined: String, fields: Int): List<List<String>> =
            joined.split('\n')
                .filter { it.isNotEmpty() }
                .map { it.split('\t', limit = fields) }
                .filter { it.size == fields }
    }
}
