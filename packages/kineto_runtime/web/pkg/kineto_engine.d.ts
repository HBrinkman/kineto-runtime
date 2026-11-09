/* tslint:disable */
/* eslint-disable */

export class KinetoWasmPlayer {
    free(): void;
    [Symbol.dispose](): void;
    activeState(state_machine_id: string): string | undefined;
    activeTransition(state_machine_id: string): string | undefined;
    advance(delta_seconds: number): void;
    /**
     * The document's sounds, as `{ id, name, sourceName }` objects — how a
     * page maps a reported firing onto its own audio.
     */
    audioEvents(): Array<any>;
    /**
     * The artboard background as straight RGBA bytes `[r, g, b, a]`
     * (issue #233); `[0, 0, 0, 0]` is transparent.
     */
    backgroundColor(): Uint8Array;
    /**
     * Maps a point on a canvas of `surface_width` × `surface_height` pixels
     * (what `renderToCanvas` drew into) to document space, for
     * `pointerEvent`. Returns `[x, y]`.
     */
    documentPoint(x: number, y: number, surface_width: number, surface_height: number): Float64Array;
    hitTestDocumentPoint(x: number, y: number): string | undefined;
    lastError(): string;
    constructor(bytes: Uint8Array);
    /**
     * One evaluated scalar of the current frame, e.g.
     * `overrideScalar("target-hand", "ikTargetPositionX")`.
     *
     * Exists for cross-runtime parity checks: hosts normally consume the
     * frame by rendering it, but a fixture has to be able to state where the
     * rig actually ended up.
     */
    overrideScalar(node_id: string, key: string): number | undefined;
    /**
     * Deliver a pointer event at a document point; returns how many
     * listeners fired (issue #74).
     */
    pointerEvent(event: string, x: number, y: number): number;
    renderToCanvas(canvas_id: string): void;
    setBool(state_machine_id: string, input_name: string, value: boolean): void;
    setNumber(state_machine_id: string, input_name: string, value: number): void;
    setTrigger(state_machine_id: string, input_name: string): void;
    /**
     * Take the sounds fired since the last call, as an array of event ids
     * (issue #111).
     *
     * A `.kvk` carries no audio bytes: the runtime says *which* sound fired
     * and the page plays whatever it ships for that event. Draining, so a
     * requestAnimationFrame loop never replays the same firing, and never
     * de-duplicated — the same sound twice is two firings.
     */
    takeFiredAudioEvents(): Array<any>;
    transitionProgress(state_machine_id: string): number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly kineto_active_audio_clips: (a: number) => number;
    readonly kineto_advance: (a: number, b: number) => number;
    readonly kineto_animations: (a: number) => number;
    readonly kineto_audio_assets: (a: number) => number;
    readonly kineto_audio_events: (a: number) => number;
    readonly kineto_background_color: (a: number, b: number) => number;
    readonly kineto_destroy: (a: number) => void;
    readonly kineto_document_point: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => number;
    readonly kineto_get_active_state: (a: number, b: number, c: number, d: number) => number;
    readonly kineto_get_active_transition: (a: number, b: number, c: number, d: number) => number;
    readonly kineto_get_transition_progress: (a: number, b: number, c: number) => number;
    readonly kineto_hit_test_document_point: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly kineto_last_error_message: (a: number) => number;
    readonly kineto_load: (a: number, b: number, c: number) => number;
    readonly kineto_machine_inputs: (a: number, b: number) => number;
    readonly kineto_override_scalar: (a: number, b: number, c: number, d: number) => number;
    readonly kineto_pause: (a: number) => number;
    readonly kineto_play: (a: number) => number;
    readonly kineto_pointer_event: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly kineto_pointer_moved: (a: number, b: number, c: number) => number;
    readonly kineto_render: (a: number, b: number) => number;
    readonly kineto_retain: (a: number) => number;
    readonly kineto_select_animation: (a: number, b: number) => number;
    readonly kineto_set_bool: (a: number, b: number, c: number, d: number) => number;
    readonly kineto_set_number: (a: number, b: number, c: number, d: number) => number;
    readonly kineto_set_trigger: (a: number, b: number, c: number) => number;
    readonly kineto_state_machines: (a: number) => number;
    readonly kineto_surface_create_from_android_native_window: (a: number, b: number, c: number, d: number) => number;
    readonly kineto_surface_create_from_ca_metal_layer: (a: number, b: number, c: number, d: number) => number;
    readonly kineto_surface_destroy: (a: number) => void;
    readonly kineto_surface_last_error_message: () => number;
    readonly kineto_surface_resize: (a: number, b: number, c: number) => number;
    readonly kineto_take_fired_audio_events: (a: number, b: number) => number;
    readonly __wbg_kinetowasmplayer_free: (a: number, b: number) => void;
    readonly kinetowasmplayer_activeState: (a: number, b: number, c: number) => [number, number];
    readonly kinetowasmplayer_activeTransition: (a: number, b: number, c: number) => [number, number];
    readonly kinetowasmplayer_advance: (a: number, b: number) => [number, number];
    readonly kinetowasmplayer_audioEvents: (a: number) => [number, number, number];
    readonly kinetowasmplayer_backgroundColor: (a: number) => [number, number, number, number];
    readonly kinetowasmplayer_documentPoint: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly kinetowasmplayer_hitTestDocumentPoint: (a: number, b: number, c: number) => [number, number];
    readonly kinetowasmplayer_lastError: (a: number) => [number, number];
    readonly kinetowasmplayer_new: (a: number, b: number) => [number, number, number];
    readonly kinetowasmplayer_overrideScalar: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly kinetowasmplayer_pointerEvent: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly kinetowasmplayer_renderToCanvas: (a: number, b: number, c: number) => [number, number];
    readonly kinetowasmplayer_setBool: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly kinetowasmplayer_setNumber: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly kinetowasmplayer_setTrigger: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly kinetowasmplayer_takeFiredAudioEvents: (a: number) => any;
    readonly kinetowasmplayer_transitionProgress: (a: number, b: number, c: number) => number;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
