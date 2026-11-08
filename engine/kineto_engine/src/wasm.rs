use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::hierarchy::{multiply, Affine};
use crate::player::KinetoPlayer;
use crate::render::{
    build_render_scene, fit_transform, RenderColor, RenderCommand, RenderPathContour, RenderScene,
    RenderShape, RenderTransform,
};

#[wasm_bindgen]
pub struct KinetoWasmPlayer {
    player: KinetoPlayer,
    last_error: String,
}

#[wasm_bindgen]
impl KinetoWasmPlayer {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<KinetoWasmPlayer, JsValue> {
        match KinetoPlayer::load(bytes.to_vec()) {
            Ok(mut player) => {
                player.play().map_err(js_error)?;
                Ok(KinetoWasmPlayer {
                    player,
                    last_error: String::new(),
                })
            }
            Err(error) => Err(js_error(error)),
        }
    }

    pub fn advance(&mut self, delta_seconds: f64) -> Result<(), JsValue> {
        match self.player.advance(delta_seconds) {
            Ok(_) => {
                self.last_error.clear();
                Ok(())
            }
            Err(error) => {
                self.last_error = error.to_string();
                Err(js_error(error))
            }
        }
    }

    #[wasm_bindgen(js_name = setBool)]
    pub fn set_bool(
        &mut self,
        state_machine_id: &str,
        input_name: &str,
        value: bool,
    ) -> Result<(), JsValue> {
        match self.player.set_bool(state_machine_id, input_name, value) {
            Ok(()) => {
                self.last_error.clear();
                Ok(())
            }
            Err(error) => {
                self.last_error = error.clone();
                Err(JsValue::from_str(&error))
            }
        }
    }

    /// Deliver a pointer event at a document point; returns how many
    /// listeners fired (issue #74).
    #[wasm_bindgen(js_name = pointerEvent)]
    pub fn pointer_event(&mut self, event: &str, x: f64, y: f64) -> Result<usize, JsValue> {
        let kind = match event {
            "down" => crate::format::ListenerEventKind::PointerDown,
            "up" => crate::format::ListenerEventKind::PointerUp,
            "enter" => crate::format::ListenerEventKind::PointerEnter,
            "exit" => crate::format::ListenerEventKind::PointerExit,
            other => {
                let message = format!("Unknown pointer event \"{other}\"");
                self.last_error = message.clone();
                return Err(JsValue::from_str(&message));
            }
        };

        match self.player.pointer_event(kind, x, y) {
            Ok(fired) => {
                self.last_error.clear();
                Ok(fired.len())
            }
            Err(error) => {
                self.last_error = error.to_string();
                Err(JsValue::from_str(&self.last_error))
            }
        }
    }

    /// Take the sounds fired since the last call, as an array of event ids
    /// (issue #111).
    ///
    /// A `.kvk` carries no audio bytes: the runtime says *which* sound fired
    /// and the page plays whatever it ships for that event. Draining, so a
    /// requestAnimationFrame loop never replays the same firing, and never
    /// de-duplicated — the same sound twice is two firings.
    #[wasm_bindgen(js_name = takeFiredAudioEvents)]
    pub fn take_fired_audio_events(&mut self) -> js_sys::Array {
        let array = js_sys::Array::new();
        for id in self.player.take_fired_audio_events() {
            array.push(&JsValue::from_str(&id));
        }
        array
    }

    /// The document's sounds, as `{ id, name, sourceName }` objects — how a
    /// page maps a reported firing onto its own audio.
    #[wasm_bindgen(js_name = audioEvents)]
    pub fn audio_events(&self) -> Result<js_sys::Array, JsValue> {
        let events = self
            .player
            .audio_events()
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let array = js_sys::Array::new();
        for (id, name, source) in events {
            let entry = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("id"),
                &JsValue::from_str(&id),
            );
            let _ = js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("name"),
                &JsValue::from_str(&name),
            );
            let _ = js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("sourceName"),
                &match source {
                    Some(value) => JsValue::from_str(&value),
                    None => JsValue::NULL,
                },
            );
            array.push(&entry);
        }
        Ok(array)
    }

    #[wasm_bindgen(js_name = setNumber)]
    pub fn set_number(
        &mut self,
        state_machine_id: &str,
        input_name: &str,
        value: f64,
    ) -> Result<(), JsValue> {
        match self.player.set_number(state_machine_id, input_name, value) {
            Ok(()) => {
                self.last_error.clear();
                Ok(())
            }
            Err(error) => {
                self.last_error = error.clone();
                Err(JsValue::from_str(&error))
            }
        }
    }

    #[wasm_bindgen(js_name = setTrigger)]
    pub fn set_trigger(&mut self, state_machine_id: &str, input_name: &str) -> Result<(), JsValue> {
        match self.player.set_trigger(state_machine_id, input_name) {
            Ok(()) => {
                self.last_error.clear();
                Ok(())
            }
            Err(error) => {
                self.last_error = error.clone();
                Err(JsValue::from_str(&error))
            }
        }
    }

    #[wasm_bindgen(js_name = activeState)]
    pub fn active_state(&self, state_machine_id: &str) -> Option<String> {
        self.player
            .active_state(state_machine_id)
            .map(str::to_owned)
    }

    #[wasm_bindgen(js_name = activeTransition)]
    pub fn active_transition(&self, state_machine_id: &str) -> Option<String> {
        self.player
            .active_transition(state_machine_id)
            .map(str::to_owned)
    }

    #[wasm_bindgen(js_name = transitionProgress)]
    pub fn transition_progress(&self, state_machine_id: &str) -> f64 {
        self.player
            .transition_progress(state_machine_id)
            .unwrap_or(0.0)
    }

    /// One evaluated scalar of the current frame, e.g.
    /// `overrideScalar("target-hand", "ikTargetPositionX")`.
    ///
    /// Exists for cross-runtime parity checks: hosts normally consume the
    /// frame by rendering it, but a fixture has to be able to state where the
    /// rig actually ended up.
    #[wasm_bindgen(js_name = overrideScalar)]
    pub fn override_scalar(&self, node_id: &str, key: &str) -> Option<f64> {
        self.player
            .current_frame()
            .ok()?
            .node_overrides
            .get(node_id)?
            .scalar(key)
    }

    /// The artboard background as straight RGBA bytes `[r, g, b, a]`
    /// (issue #233); `[0, 0, 0, 0]` is transparent.
    #[wasm_bindgen(js_name = backgroundColor)]
    pub fn background_color(&self) -> Result<Vec<u8>, JsValue> {
        self.player
            .background_color()
            .map(|rgba| rgba.to_vec())
            .map_err(js_error)
    }

    /// Maps a point on a canvas of `surface_width` × `surface_height` pixels
    /// (what `renderToCanvas` drew into) to document space, for
    /// `pointerEvent`. Returns `[x, y]`.
    #[wasm_bindgen(js_name = documentPoint)]
    pub fn document_point(
        &self,
        x: f64,
        y: f64,
        surface_width: f64,
        surface_height: f64,
    ) -> Result<Vec<f64>, JsValue> {
        let frame = self.player.current_frame().map_err(js_error)?;
        let document = self.player.document().map_err(js_error)?;
        let scene = build_render_scene(document, &frame);
        let base = surface_transform(&scene, surface_width, surface_height);
        let (dx, dy) = match crate::hierarchy::invert(base) {
            Some(inverse) => crate::hierarchy::apply_point(inverse, x, y),
            None => (x, y),
        };
        Ok(vec![dx, dy])
    }

    #[wasm_bindgen(js_name = hitTestDocumentPoint)]
    pub fn hit_test_document_point(&self, x: f64, y: f64) -> Option<String> {
        self.player
            .hit_test_document_point(x, y)
            .ok()
            .flatten()
            .map(|target| target.node_id)
    }

    #[wasm_bindgen(js_name = renderToCanvas)]
    pub fn render_to_canvas(&mut self, canvas_id: &str) -> Result<(), JsValue> {
        let frame = self.player.current_frame().map_err(js_error)?;
        let document = self.player.document().map_err(js_error)?;
        let scene = build_render_scene(document, &frame);
        render_scene_to_canvas(canvas_id, &scene).map_err(|error| {
            self.last_error = error;
            JsValue::from_str(&self.last_error)
        })?;
        self.last_error.clear();
        Ok(())
    }

    #[wasm_bindgen(js_name = lastError)]
    pub fn last_error(&self) -> String {
        self.last_error.clone()
    }
}

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn render_scene_to_canvas(canvas_id: &str, scene: &RenderScene) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "window unavailable".to_owned())?;
    let document = window
        .document()
        .ok_or_else(|| "document unavailable".to_owned())?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| format!("canvas \"{canvas_id}\" not found"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element \"{canvas_id}\" is not a canvas"))?;
    if canvas.width() == 0 {
        canvas.set_width(640);
    }
    if canvas.height() == 0 {
        canvas.set_height(360);
    }
    let context = canvas
        .get_context("2d")
        .map_err(|_| "unable to get 2d context".to_owned())?
        .ok_or_else(|| "2d context unavailable".to_owned())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "context is not CanvasRenderingContext2D".to_owned())?;

    let width = canvas.width() as f64;
    let height = canvas.height() as f64;
    // Transparent, like the device surfaces: the host page decides what is
    // behind the artwork.
    context.clear_rect(0.0, 0.0, width, height);
    // The artboard background (issue #233): the whole canvas, letterbox
    // included. Canvas2D takes straight alpha and premultiplies itself.
    if let Some([r, g, b, a]) = scene.background {
        context.set_fill_style_str(&format!(
            "rgba({r},{g},{b},{})",
            f64::from(a) / 255.0
        ));
        context.fill_rect(0.0, 0.0, width, height);
    }

    let base = surface_transform(scene, width, height);
    context.save();
    for command in &scene.commands {
        draw_command(&context, command, base);
    }
    context.restore();
    Ok(())
}

/// Document space to canvas pixels: the artboard fitted into the canvas,
/// through the camera, exactly as the vello surfaces draw it
/// (`fit_transform` · camera), so a point maps the same way on every
/// platform and `documentPoint` can undo it.
///
/// A document without a canvas size falls back to fitting the artwork's
/// bounds, which is what this preview always did.
fn surface_transform(scene: &RenderScene, width: f64, height: f64) -> Affine {
    let (cw, ch) = scene.canvas;
    if cw > 0.0 && ch > 0.0 {
        return multiply(fit_transform(scene.canvas, (width, height)), scene.camera);
    }
    let fit = Fit::for_scene(scene, width, height);
    [fit.scale, 0.0, 0.0, fit.scale, fit.offset_x, fit.offset_y]
}

fn set_affine(context: &web_sys::CanvasRenderingContext2d, m: Affine) {
    let _ = context.set_transform(m[0], m[1], m[2], m[3], m[4], m[5]);
}

/// Applies a node's group chain and its own transform on top of `base`.
fn apply_node_transform(
    context: &web_sys::CanvasRenderingContext2d,
    base: Affine,
    parent: Affine,
    transform: RenderTransform,
) {
    set_affine(context, multiply(base, parent));
    let _ = context.translate(transform.x, transform.y);
    let _ = context.translate(transform.anchor_x, transform.anchor_y);
    let _ = context.rotate(transform.rotation);
    let _ = context.scale(transform.scale_x, transform.scale_y);
    let _ = context.translate(-transform.anchor_x, -transform.anchor_y);
}

fn draw_command(
    context: &web_sys::CanvasRenderingContext2d,
    command: &RenderCommand,
    base: Affine,
) {
    context.save();
    context.set_global_alpha(command.opacity);
    // The mask's outline, in the mask's own space, becomes the clip. A clip
    // survives the transform change back to this node's space.
    if let Some(clip) = &command.clip {
        apply_node_transform(context, base, clip.parent, clip.transform);
        context.begin_path();
        trace_shape(context, &clip.shape);
        context.clip();
    }
    // Every group above this node, then the node's own transform.
    apply_node_transform(context, base, command.parent, command.transform);

    context.begin_path();
    trace_shape(context, &command.shape);

    for fill in &command.fills {
        // canvas2d can draw real gradients, but this preview surface has never
        // drawn one at all — the fill reader returned None for anything but a
        // solid. Collapsing to the first stop is the documented fallback and a
        // strict improvement on an invisible shape; the vello backend, which
        // is what ships on device, draws the gradient properly.
        context.set_fill_style_str(&css_color(fill.approximate_color()));
        context.fill();
    }
    if let Some(stroke) = &command.stroke {
        context.set_stroke_style_str(&css_color(stroke.color));
        context.set_line_width(stroke.width.max(0.0));
        context.stroke();
    }
    context.restore();
}

fn trace_shape(context: &web_sys::CanvasRenderingContext2d, shape: &RenderShape) {
    match shape {
        RenderShape::Rectangle { width, height, .. } => context.rect(0.0, 0.0, *width, *height),
        RenderShape::Ellipse { width, height } => {
            let _ = context.ellipse(
                width / 2.0,
                height / 2.0,
                width.abs() / 2.0,
                height.abs() / 2.0,
                0.0,
                0.0,
                std::f64::consts::TAU,
            );
        }
        RenderShape::Path { contours, .. } => {
            for contour in contours {
                draw_contour(context, contour);
            }
        }
        // The pixels are decoded and available; putting them on a canvas2d
        // context needs an ImageData round trip this preview does not do yet,
        // so it outlines the frame. That keeps the layout honest — an image
        // reads as a gap rather than shifting everything around it — and the
        // vello backend, which is what ships on device, draws the bitmap.
        RenderShape::Image { width, height, .. } => {
            context.rect(0.0, 0.0, *width, *height);
        }
        // Glyph outlines are laid out but not assembled into a canvas2d path.
        // This preview draws nothing for them; the vello backend, which is
        // what ships on device, rasterises them.
        RenderShape::Text { .. } => {}
    }
}

fn draw_contour(context: &web_sys::CanvasRenderingContext2d, contour: &RenderPathContour) {
    let Some(first) = contour.points.first() else {
        return;
    };
    context.move_to(first.anchor_x, first.anchor_y);
    for pair in contour.points.windows(2) {
        let from = pair[0];
        let to = pair[1];
        context.bezier_curve_to(
            from.anchor_x + from.out_handle_x,
            from.anchor_y + from.out_handle_y,
            to.anchor_x + to.in_handle_x,
            to.anchor_y + to.in_handle_y,
            to.anchor_x,
            to.anchor_y,
        );
    }
    if contour.closed {
        // The closing segment has handles too (`path_segments`). Closing with
        // a straight line cut a wedge off every outline whose last segment
        // curves back to the start.
        if let (Some(last), true) = (contour.points.last(), contour.points.len() > 2) {
            context.bezier_curve_to(
                last.anchor_x + last.out_handle_x,
                last.anchor_y + last.out_handle_y,
                first.anchor_x + first.in_handle_x,
                first.anchor_y + first.in_handle_y,
                first.anchor_x,
                first.anchor_y,
            );
        }
        context.close_path();
    }
}

fn css_color(color: RenderColor) -> String {
    format!(
        "rgba({}, {}, {}, {:.4})",
        color.r,
        color.g,
        color.b,
        color.a as f64 / 255.0
    )
}

#[derive(Debug, Clone, Copy)]
struct Fit {
    offset_x: f64,
    offset_y: f64,
    scale: f64,
}

impl Fit {
    fn for_scene(scene: &RenderScene, width: f64, height: f64) -> Self {
        let mut bounds = Bounds::default();
        for command in &scene.commands {
            bounds.include_command(command);
        }
        let Some((min_x, min_y, max_x, max_y)) = bounds.into_tuple() else {
            return Self {
                offset_x: 0.0,
                offset_y: 0.0,
                scale: 1.0,
            };
        };
        let scene_width = (max_x - min_x).max(1.0);
        let scene_height = (max_y - min_y).max(1.0);
        let scale = (width / scene_width).min(height / scene_height) * 0.8;
        let offset_x = (width - scene_width * scale) / 2.0 - min_x * scale;
        let offset_y = (height - scene_height * scale) / 2.0 - min_y * scale;
        Self {
            offset_x,
            offset_y,
            scale,
        }
    }
}

#[derive(Default)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    has_value: bool,
}

impl Bounds {
    fn include_command(&mut self, command: &RenderCommand) {
        let t = command.transform;
        // Through the groups above this node as well, or a grouped drawing
        // would size the canvas from where it is not (issue #74).
        let mut include_local = |x: f64, y: f64| {
            let (x, y) = crate::hierarchy::apply_point(command.parent, x, y);
            self.include(x, y);
        };
        match &command.shape {
            RenderShape::Rectangle { width, height, .. }
            | RenderShape::Ellipse { width, height }
            | RenderShape::Image { width, height, .. } => {
                include_local(t.x, t.y);
                include_local(t.x + width * t.scale_x, t.y + height * t.scale_y);
            }
            // Sized from the glyph pen positions, so a text node still
            // contributes to the canvas the preview sizes to itself.
            RenderShape::Text {
                font_size, glyphs, ..
            } => {
                for glyph in glyphs {
                    include_local(t.x + glyph.x * t.scale_x, t.y + glyph.y * t.scale_y);
                    include_local(
                        t.x + glyph.x * t.scale_x,
                        t.y + (glyph.y + font_size) * t.scale_y,
                    );
                }
            }
            RenderShape::Path { contours, .. } => {
                for contour in contours {
                    for point in &contour.points {
                        include_local(
                            t.x + point.anchor_x * t.scale_x,
                            t.y + point.anchor_y * t.scale_y,
                        );
                    }
                }
            }
        }
    }

    fn include(&mut self, x: f64, y: f64) {
        if !self.has_value {
            self.min_x = x;
            self.min_y = y;
            self.max_x = x;
            self.max_y = y;
            self.has_value = true;
            return;
        }
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn into_tuple(self) -> Option<(f64, f64, f64, f64)> {
        self.has_value
            .then_some((self.min_x, self.min_y, self.max_x, self.max_y))
    }
}
