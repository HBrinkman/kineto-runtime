package app.kineto.runtime

import android.view.Surface

internal object KinetoRuntimeJni {
    init {
        System.loadLibrary("kineto_engine")
    }

    @JvmStatic external fun nativeLoad(bytes: ByteArray, outStatus: IntArray): Long
    @JvmStatic external fun nativeDestroy(handle: Long)
    @JvmStatic external fun nativeRetain(handle: Long): Int
    @JvmStatic external fun nativeAdvance(handle: Long, deltaSeconds: Double): Int
    @JvmStatic external fun nativeSetBool(
        handle: Long,
        stateMachineId: String,
        inputName: String,
        value: Boolean,
    ): Int
    @JvmStatic external fun nativeSetNumber(
        handle: Long,
        stateMachineId: String,
        inputName: String,
        value: Double,
    ): Int
    @JvmStatic external fun nativePointerEvent(
        handle: Long,
        event: Int,
        x: Double,
        y: Double,
    ): Int
    @JvmStatic external fun nativePointerMoved(handle: Long, x: Double, y: Double): Int
    @JvmStatic external fun nativeBackgroundColor(handle: Long, outRgba: IntArray): Int
    @JvmStatic external fun nativeDocumentPoint(
        handle: Long,
        surfaceWidth: Double,
        surfaceHeight: Double,
        x: Double,
        y: Double,
        outPoint: DoubleArray,
    ): Int
    @JvmStatic external fun nativeOverrideScalar(
        handle: Long,
        nodeId: String,
        key: String,
        outValue: DoubleArray,
    ): Int
    @JvmStatic external fun nativePlay(handle: Long): Int
    @JvmStatic external fun nativePause(handle: Long): Int
    @JvmStatic external fun nativeSelectAnimation(handle: Long, animationId: String): Int
    @JvmStatic external fun nativeStateMachines(handle: Long): String
    @JvmStatic external fun nativeMachineInputs(handle: Long, machineId: String): String
    @JvmStatic external fun nativeAnimations(handle: Long): String
    @JvmStatic external fun nativeAudioAssets(handle: Long): String
    @JvmStatic external fun nativeActiveAudioClips(handle: Long): String
    @JvmStatic external fun nativeTakeFiredAudioEvents(handle: Long): String
    @JvmStatic external fun nativeAudioEvents(handle: Long): String
    @JvmStatic external fun nativeSetTrigger(
        handle: Long,
        stateMachineId: String,
        inputName: String,
    ): Int
    @JvmStatic external fun nativeActiveState(handle: Long, stateMachineId: String): String?
    @JvmStatic external fun nativeActiveTransition(handle: Long, stateMachineId: String): String?
    @JvmStatic external fun nativeTransitionProgress(
        handle: Long,
        stateMachineId: String,
        outStatus: IntArray,
    ): Double
    @JvmStatic external fun nativeHitTestDocumentPoint(handle: Long, x: Double, y: Double): String?
    @JvmStatic external fun nativeLastError(handle: Long): String
    @JvmStatic external fun nativeAttachSurface(surface: Surface, width: Int, height: Int): Long
    @JvmStatic external fun nativeDestroySurface(surfaceHandle: Long)
    @JvmStatic external fun nativeResizeSurface(surfaceHandle: Long, width: Int, height: Int): Int
    @JvmStatic external fun nativeRender(handle: Long, surfaceHandle: Long): Int
    @JvmStatic external fun nativeSurfaceLastError(): String
}
