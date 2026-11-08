package app.kineto.runtime

/** Mirrors `KinetoStatus` in the C ABI; the ordinal is the wire value. */
enum class KinetoStatus {
    OK,
    NULL_POINTER,
    INVALID_ARGUMENT,
    LOAD_FAILED,
    ADVANCE_FAILED,
    RENDER_FAILED,
    UNSUPPORTED,
    ;

    companion object {
        /** An unknown code is an error, never a success. */
        fun fromCode(code: Int): KinetoStatus =
            entries.getOrNull(code)?.takeIf { code >= 0 } ?: INVALID_ARGUMENT
    }
}

class KinetoRuntimeException(
    val status: KinetoStatus,
    message: String,
) : RuntimeException(message)
