use std::collections::{HashMap, HashSet};

use crate::bones::{
    apply_skeletal_transforms, bones_from_document, matrix4_from_affine, solve_fk,
    RuntimePathPoint, IDENTITY_MATRIX4,
};
use crate::format;
use crate::format::generated::kineto::{Component, Group};
use crate::hierarchy::{parent_map, Affine, IDENTITY};
use crate::player::EvaluatedFrame;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RenderScene {
    pub commands: Vec<RenderCommand>,
    /// The authored document size, in document units.
    ///
    /// The exporter has always written this and the engine never read it, so
    /// the scene was drawn 1:1 in pixels — a 1920x1080 document on a larger
    /// drawable sat in the corner at the wrong size instead of being fitted
    /// (issue #111 parity audit §5.10).
    pub canvas: (f64, f64),
    /// The virtual camera the whole document is drawn through, already
    /// composed. Identity when the document has none.
    pub camera: Affine,
    /// The artboard background (issue #233, ADR-0011): straight, not
    /// premultiplied, RGBA. The whole surface is cleared to it before any
    /// node is painted. `None` is transparent — every file written before the
    /// field existed, and any whose alpha is 0.
    pub background: Option<[u8; 4]>,
}

/// The background a document clears to, straight RGBA; `None` when absent or
/// fully transparent so those two can never render differently.
pub fn background_of(document: &format::Document<'_>) -> Option<[u8; 4]> {
    let color = document.background()?;
    (color.a() > 0).then(|| [color.r(), color.g(), color.b(), color.a()])
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderCommand {
    pub node_id: String,
    pub node_name: String,
    pub opacity: f64,
    /// The composed transform of every group above this node (issue #74).
    /// Identity for a top-level node. Groups were exported all along and
    /// nothing read them, so a group with a transform drew its contents at
    /// the wrong place.
    pub parent: Affine,
    pub transform: RenderTransform,
    pub shape: RenderShape,
    /// Every fill, bottom-most first.
    ///
    /// The editor stacks fills on one node — a solid under a gradient is how a
    /// tint is built — and only the first one was ever drawn. Empty means an
    /// unfilled shape.
    pub fills: Vec<RenderPaint>,
    pub stroke: Option<RenderStroke>,
    /// Drop shadows behind this command, in the order they were authored.
    ///
    /// `RenderCommand` had no effects member at all, so a shape with a shadow
    /// drew flat on device (issue #111). Blur is not carried: the editor
    /// blurs its content through a save-layer image filter, and vello has no
    /// general blur — so a blur effect is recorded as unsupported rather than
    /// approximated into something that looks like a different mistake.
    pub shadows: Vec<RenderShadow>,
    /// The mask that clips this command, if an enclosing group has one.
    ///
    /// `GroupNode.maskChildId` had no schema field until issue #111, so masked
    /// artwork exported and drew unmasked. Carried per command rather than by
    /// nesting the scene, because `RenderScene` is a flat list that two
    /// backends already walk; a backend clips, draws and unclips around each
    /// command it sees one on.
    pub clip: Option<RenderClip>,
    /// How this node composites against what is already drawn.
    ///
    /// The editor has had 16 modes since M11 and the schema had none, so every
    /// node composited as Normal on device (issue #111).
    pub blend_mode: RenderBlendMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderBlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// The render blend mode for a schema value.
///
/// Unknown values composite as Normal, which is the schema default and the
/// only answer that cannot make a node disappear. `tests/schema_coverage.rs`
/// fails if a mode reaches this fallback, so a mode added to the schema is
/// caught rather than silently flattened.
pub fn blend_mode_from_kineto(
    mode: format::generated::kineto::BlendMode,
) -> Option<RenderBlendMode> {
    use format::generated::kineto::BlendMode as Fb;
    Some(match mode {
        Fb::Normal => RenderBlendMode::Normal,
        Fb::Multiply => RenderBlendMode::Multiply,
        Fb::Screen => RenderBlendMode::Screen,
        Fb::Overlay => RenderBlendMode::Overlay,
        Fb::Darken => RenderBlendMode::Darken,
        Fb::Lighten => RenderBlendMode::Lighten,
        Fb::ColorDodge => RenderBlendMode::ColorDodge,
        Fb::ColorBurn => RenderBlendMode::ColorBurn,
        Fb::HardLight => RenderBlendMode::HardLight,
        Fb::SoftLight => RenderBlendMode::SoftLight,
        Fb::Difference => RenderBlendMode::Difference,
        Fb::Exclusion => RenderBlendMode::Exclusion,
        Fb::Hue => RenderBlendMode::Hue,
        Fb::Saturation => RenderBlendMode::Saturation,
        Fb::Color => RenderBlendMode::Color,
        Fb::Luminosity => RenderBlendMode::Luminosity,
        _ => return None,
    })
}

/// A drop shadow, in the node's own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderShadow {
    pub color: RenderColor,
    pub offset_x: f64,
    pub offset_y: f64,
    /// The authored blur radius. The renderer halves it for a gaussian
    /// standard deviation, matching `frame_renderer.dart:653`.
    pub blur: f64,
    pub spread: f64,
}

/// A mask shape, already positioned in the same space as the command it clips.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderClip {
    pub node_id: String,
    pub shape: RenderShape,
    pub parent: Affine,
    pub transform: RenderTransform,
}

/// How an area is painted.
///
/// Gradient geometry is stored **normalised to the node's bounding box**, the
/// way the editor authors and draws it (`frame_renderer.dart:733-765`): 0..1
/// across the box for the linear endpoints and the radial centre and focus,
/// and a radius scaled by the box's longer side. Keeping it normalised here
/// rather than resolving against bounds in `build_render_scene` is what lets
/// the vello and canvas2d backends each resolve it in their own space, and
/// keeps the render scene independent of how a shape computes its bounds.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderPaint {
    Solid(RenderColor),
    LinearGradient {
        stops: Vec<RenderGradientStop>,
        from: (f64, f64),
        to: (f64, f64),
        spread: RenderSpread,
    },
    RadialGradient {
        stops: Vec<RenderGradientStop>,
        center: (f64, f64),
        focal: (f64, f64),
        /// A fraction of the bounding box's longer side.
        radius: f64,
        spread: RenderSpread,
    },
}

impl RenderPaint {
    /// The paint as a single colour, for a backend that cannot draw gradients.
    ///
    /// A gradient collapses to its first stop. That is a visible
    /// approximation, and it is deliberately not what the vello backend does —
    /// it exists so a fallback surface draws *something* recognisable instead
    /// of nothing, which is what a gradient used to get.
    pub fn approximate_color(&self) -> RenderColor {
        match self {
            RenderPaint::Solid(color) => *color,
            RenderPaint::LinearGradient { stops, .. }
            | RenderPaint::RadialGradient { stops, .. } => stops
                .first()
                .map(|stop| stop.color)
                .unwrap_or(RenderColor { r: 0, g: 0, b: 0, a: 0 }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderGradientStop {
    /// 0..1 along the gradient.
    pub offset: f64,
    pub color: RenderColor,
}

/// What the gradient does outside its own 0..1 range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSpread {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderTransform {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

impl Default for RenderTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            anchor_x: 0.0,
            anchor_y: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderShape {
    Rectangle {
        width: f64,
        height: f64,
        /// Uniform corner radius. The editor and the Metal renderer both draw
        /// it; the runtime used to ignore it and draw sharp corners.
        corner_radius: f64,
    },
    Ellipse {
        width: f64,
        height: f64,
    },
    Path {
        contours: Vec<RenderPathContour>,
        /// True for even-odd, false for non-zero. The runtime used to fill
        /// every path non-zero, so an authored even-odd hole filled solid —
        /// while the Metal renderer honoured the document. Three renderers,
        /// three answers.
        even_odd: bool,
    },
    /// A bitmap, drawn into `0..width x 0..height` in the node's own space.
    ///
    /// The PNG bytes have shipped in every `.kvk` since image nodes existed
    /// and nothing drew them (issue #111), so every image was dead weight in
    /// the package and a hole in the artwork.
    Image {
        width: f64,
        height: f64,
        /// Premultiplied-alpha RGBA8, row-major.
        rgba: Vec<u8>,
        pixel_width: u32,
        pixel_height: u32,
    },
    /// Laid-out text, ready for the glyph rasteriser.
    ///
    /// Text nodes exported and drew nothing for want of a font. The face
    /// travels with the document (`FontFace` in the schema), and this carries
    /// the glyphs already positioned in the node's own space.
    Text {
        /// The whole TTF/OTF, as shipped in the document.
        font_data: Vec<u8>,
        font_size: f64,
        glyphs: Vec<RenderGlyph>,
    },
}

/// One positioned glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderGlyph {
    pub id: u32,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPathContour {
    pub closed: bool,
    pub points: Vec<RenderPathPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPathPoint {
    pub anchor_x: f64,
    pub anchor_y: f64,
    pub in_handle_x: f64,
    pub in_handle_y: f64,
    pub out_handle_x: f64,
    pub out_handle_y: f64,
}

/// One cubic segment of a path, in the shape a renderer needs: absolute
/// points, control handles already resolved against their anchors.
///
/// Split out of the vello conversion so it can be checked without a GPU —
/// and, more to the point, so the runtime's reading of a path can be compared
/// against the editor's. Four implementations of this rule exist (the editor's
/// Dart and Metal renderers, the web runtime and this one) and one of them
/// silently disagreed for months; see `tests/path_parity.rs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicSegment {
    pub start: (f64, f64),
    pub control1: (f64, f64),
    pub control2: (f64, f64),
    pub end: (f64, f64),
}

/// One contour, decomposed into cubics.
#[derive(Debug, Clone, PartialEq)]
pub struct ContourSegments {
    pub closed: bool,
    pub segments: Vec<CubicSegment>,
}

/// Turns authored path points into absolute cubic segments.
///
/// **Handles are relative to their anchor.** A handle of (0,0) means the
/// segment leaves straight, and is emitted as a cubic whose controls sit on
/// its endpoints — one shape for every segment, so a comparison never has to
/// care whether a reader called something a line or a curve.
pub fn path_segments(contours: &[RenderPathContour]) -> Vec<ContourSegments> {
    contours
        .iter()
        .filter(|contour| !contour.points.is_empty())
        .map(|contour| {
            let mut segments = Vec::new();
            for pair in contour.points.windows(2) {
                segments.push(cubic_between(pair[0], pair[1]));
            }
            if contour.closed && contour.points.len() > 2 {
                // The closing segment is a segment like any other: its
                // handles matter too.
                segments.push(cubic_between(
                    *contour.points.last().expect("non-empty"),
                    contour.points[0],
                ));
            }
            ContourSegments {
                closed: contour.closed,
                segments,
            }
        })
        .collect()
}

fn cubic_between(previous: RenderPathPoint, next: RenderPathPoint) -> CubicSegment {
    CubicSegment {
        start: (previous.anchor_x, previous.anchor_y),
        control1: (
            previous.anchor_x + previous.out_handle_x,
            previous.anchor_y + previous.out_handle_y,
        ),
        control2: (next.anchor_x + next.in_handle_x, next.anchor_y + next.in_handle_y),
        end: (next.anchor_x, next.anchor_y),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderStroke {
    pub color: RenderColor,
    pub width: f64,
    /// How the line ends. `RenderStroke` used to be `{color, width}`, so every
    /// stroke drew butt-capped and mitre-joined however it was authored.
    pub cap: RenderCap,
    pub join: RenderJoin,
    pub miter_limit: f64,
    /// Alternating on/off lengths. Empty means a solid line.
    pub dash_array: Vec<f64>,
    pub dash_offset: f64,
    /// Which side of the outline the line sits on.
    pub align: RenderStrokeAlign,
    /// Trim path (issue #158), as fractions of the outline's arc length:
    /// start and end pick the drawn span, offset slides it and wraps. The
    /// neutral `0, 1, 0` draws the whole stroke. See [`trim_windows`].
    pub trim_start: f64,
    pub trim_end: f64,
    pub trim_offset: f64,
}

/// One drawn span of a trimmed stroke, as fractions of the outline's arc
/// length. On a closed outline `to` may exceed 1: the span runs on through
/// the start point as one unbroken piece.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimWindow {
    pub from: f64,
    pub to: f64,
}

/// Spans shorter than this draw nothing rather than a dot.
const EMPTY_TRIM: f64 = 1e-6;

/// The spans of an outline a trimmed stroke draws (issue #158).
///
/// `None` means the trim is neutral: draw the outline untouched, closed
/// contours staying closed. An empty list draws nothing. Otherwise the spans
/// come in path order.
///
/// Start and end clamp to 0..1 and swap when reversed; offset wraps, so an
/// offset of 1 is no offset. On an open outline a span pushed past the end
/// comes back in at the start as a second piece; on a closed one it continues
/// through the seam. The editor runs the same rule — `strokeTrimWindows` in
/// `stroke_trim.dart` and `StrokeTrim.windows` in `StrokeTrim.swift` — and all
/// three test suites read `test/fixtures/stroke_trim_windows.txt`.
pub fn trim_windows(start: f64, end: f64, offset: f64, closed: bool) -> Option<Vec<TrimWindow>> {
    let mut s = if start.is_nan() { 0.0 } else { start.clamp(0.0, 1.0) };
    let mut e = if end.is_nan() { 1.0 } else { end.clamp(0.0, 1.0) };
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    let span = e - s;
    if span >= 1.0 - EMPTY_TRIM {
        return None;
    }
    if span <= EMPTY_TRIM {
        return Some(Vec::new());
    }

    let shift = if offset.is_finite() { offset - offset.floor() } else { 0.0 };
    let mut a = s + shift;
    let mut b = e + shift;
    if a >= 1.0 {
        a -= 1.0;
        b -= 1.0;
    }
    if b <= 1.0 || closed {
        return Some(vec![TrimWindow { from: a, to: b }]);
    }
    // Open outline, span wrapped past the end: the part past 1 starts over.
    Some(vec![
        TrimWindow { from: 0.0, to: b - 1.0 },
        TrimWindow { from: a, to: 1.0 },
    ])
}

impl RenderStroke {
    /// Whether the trim changes what this stroke draws at all.
    pub fn is_trimmed(&self) -> bool {
        trim_windows(self.trim_start, self.trim_end, self.trim_offset, true).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStrokeAlign {
    Inside,
    Center,
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderJoin {
    Miter,
    Round,
    Bevel,
}

pub fn build_render_scene(document: format::Document<'_>, frame: &EvaluatedFrame) -> RenderScene {
    let rest_fk = solve_fk(&bones_from_document(&document));
    let parents = parent_map(&document);
    let attached = crate::hierarchy::attach_worlds(&document, &frame.fk);
    let world =
        crate::hierarchy::world_affines_attached(&document, &frame.node_overrides, &attached);
    let mut commands = Vec::new();

    // A node's own visibility and opacity are not the whole story: a group
    // hides and fades what is inside it. Groups emit no draw command of their
    // own, so without this a hidden group's children still drew and a faded
    // group did not fade — the editor composites a layer per group
    // (`frame_renderer.dart:469`).
    let nodes_by_id: HashMap<&str, format::Node<'_>> = document
        .nodes()
        .unwrap_or_default()
        .iter()
        .map(|node| (node.id(), node))
        .collect();

    for node in paint_order(&document, frame) {
        if !node.visible() {
            continue;
        }

        let overrides = frame.node_overrides.get(node.id());
        let own_opacity = overrides
            .and_then(|values| values.scalar("opacity"))
            .unwrap_or(node.opacity() as f64)
            .clamp(0.0, 1.0);

        let Some(inherited) = inherited_opacity(node.id(), &parents, &nodes_by_id, frame) else {
            // An ancestor is hidden, so this node is not on screen at all.
            continue;
        };
        let opacity = (own_opacity * inherited).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }

        let (parent, transform) = match attached.get(node.id()) {
            // Attached to a bone: the bone places it outright, so its own
            // transform does not apply on top.
            Some(world) => (*world, RenderTransform::default()),
            None => (
                // Only the ancestors: the node's own transform still travels
                // as a RenderTransform, which the canvas2d path needs
                // decomposed.
                parents
                    .get(node.id())
                    .and_then(|parent_id| world.get(parent_id))
                    .copied()
                    .unwrap_or(IDENTITY),
                render_transform(node.transform(), overrides),
            ),
        };

        // Dispatched by a single match on the component tag rather than a
        // chain of `component_as_*` ifs. The chain's failure mode was silence:
        // a component it did not know about fell off the end and drew nothing,
        // which is how Text and Image shipped invisible. `component_support`
        // below is the one place that says what the runtime draws, and
        // `tests/schema_coverage.rs` holds it against the schema.
        let command = match node.component_type() {
            Component::Shape => node
                .component_as_shape()
                .and_then(|shape| {
                    shape_command(node, shape, opacity, parent, transform, overrides)
                }),
            Component::Path => node.component_as_path().and_then(|path| {
                path_command(
                    node, path, opacity, parent, transform, overrides, &frame.fk, &rest_fk,
                )
            }),
            Component::CompoundPath => node.component_as_compound_path().and_then(|path| {
                compound_path_command(
                    node, path, opacity, parent, transform, overrides, &frame.fk, &rest_fk,
                )
            }),
            // A group is a transform and a compositing scope, not a mark. Its
            // children carry the drawing.
            Component::Group | Component::NONE => None,
            Component::Image => node
                .component_as_image()
                .and_then(|image| image_command(node, image, opacity, parent, transform)),
            Component::Text => node.component_as_text().and_then(|text| {
                text_command(&document, node, text, opacity, parent, transform, overrides)
            }),
            _ => None,
        };
        if let Some(command) = command {
            commands.push(command);
        }
    }

    attach_clip_masks(&document, &mut commands);

    // The author's z-order. The runtime drew in document order and could not
    // re-sort, so a node sent forward or back in the editor did not move on
    // device (issue #111). A stable sort keeps document order as the
    // tie-break, which is what every pre-#111 file relies on since they all
    // say 0.
    if document
        .nodes()
        .unwrap_or_default()
        .iter()
        .any(|node| node.draw_order() != 0)
    {
        let order: HashMap<&str, i32> = document
            .nodes()
            .unwrap_or_default()
            .iter()
            .map(|node| (node.id(), node.draw_order()))
            .collect();
        commands.sort_by_key(|command| {
            order.get(command.node_id.as_str()).copied().unwrap_or(0)
        });
    }

    let canvas = (
        document.canvas_width() as f64,
        document.canvas_height() as f64,
    );
    RenderScene {
        commands,
        canvas,
        camera: camera_affine(&document, frame, canvas),
        background: background_of(&document),
    }
}

/// Maps a point on the drawable back into document space.
///
/// The inverse of [`fit_transform`] composed with the camera — everything the
/// renderer puts between a document coordinate and a pixel. A host that hit
/// tests has to undo exactly that, and doing the arithmetic on its side is how
/// it drifts: the surface started fitting the document into the drawable
/// (issue #111) while the reference app still multiplied the touch by the
/// layer's contents scale, so every tap landed somewhere the artwork was not.
///
/// A degenerate surface or a non-invertible camera returns the point
/// unchanged, which is the 1:1 mapping that was correct before the fit.
pub fn document_point(
    canvas: (f64, f64),
    camera: Affine,
    surface: (f64, f64),
    x: f64,
    y: f64,
) -> (f64, f64) {
    let composed = crate::hierarchy::multiply(fit_transform(canvas, surface), camera);
    match crate::hierarchy::invert(composed) {
        Some(inverse) => crate::hierarchy::apply_point(inverse, x, y),
        None => (x, y),
    }
}

/// The virtual camera, as a transform applied to the whole document.
///
/// The editor has drawn through one since M11 and the runtime had no camera at
/// all, so a document that panned or zoomed sat still on device. Camera tracks
/// target the reserved id `__camera__`, which matches no node — the runtime
/// used to hand them to a node lookup that always failed, so they did nothing
/// twice over (issue #111).
///
/// Applied about the canvas centre, which is what
/// `frame_renderer.dart:141-147` does.
pub fn camera_affine(
    document: &format::Document<'_>,
    frame: &EvaluatedFrame,
    canvas: (f64, f64),
) -> Affine {
    use crate::hierarchy::multiply;

    let base = document.camera();
    let overrides = frame.node_overrides.get(CAMERA_NODE_ID);
    let scalar = |key: &str, fallback: f64| {
        overrides
            .and_then(|values| values.scalar(key))
            .unwrap_or(fallback)
    };

    let x = scalar("cameraX", base.map(|c| c.x() as f64).unwrap_or(0.0));
    let y = scalar("cameraY", base.map(|c| c.y() as f64).unwrap_or(0.0));
    let zoom = scalar("cameraZoom", base.map(|c| c.zoom() as f64).unwrap_or(1.0));
    let rotation = scalar(
        "cameraRotation",
        base.map(|c| c.rotation() as f64).unwrap_or(0.0),
    );

    if x == 0.0 && y == 0.0 && zoom == 1.0 && rotation == 0.0 {
        return IDENTITY;
    }

    let (cx, cy) = (canvas.0 / 2.0, canvas.1 / 2.0);
    let (sin, cos) = rotation.sin_cos();
    let mut affine = [1.0, 0.0, 0.0, 1.0, cx, cy];
    affine = multiply(affine, [cos, sin, -sin, cos, 0.0, 0.0]);
    affine = multiply(affine, [zoom, 0.0, 0.0, zoom, 0.0, 0.0]);
    multiply(affine, [1.0, 0.0, 0.0, 1.0, -cx + x, -cy + y])
}

/// The reserved subject id camera tracks target.
///
/// Matches `_cameraNodeId` in `timeline_eval.dart:223`. It is deliberately not
/// a node: a camera is a property of the document, and giving it a node would
/// mean it could be grouped, hidden or masked.
pub const CAMERA_NODE_ID: &str = "__camera__";

/// Nodes back to front, the order the editor paints them (issue #148,
/// `RenderStackResolver`): root nodes in `doc.nodes` order, each group in its
/// `children` order.
///
/// `doc.nodes` alone is not that order. The exporter lists a group's members
/// before the group itself, so a root shape listed after a root group (a
/// background behind a character) drew on top of the whole group. A node
/// claimed by two groups belongs to the first, and one no walk reaches (a
/// cycle) does not draw: the editor's rules.
///
/// The frame changes the walk in two ways (issue #214), both sibling-local
/// (D5) and both the editor's own rules:
///
///  * a Swap Group walks only the child it shows ([`solo_child`]) — and its
///    clipping mask, which keeps its role;
///  * a sibling list with a keyed `drawOrder` child is re-sorted first
///    ([`sorted_siblings`]).
pub fn paint_order<'a>(
    document: &format::Document<'a>,
    frame: &EvaluatedFrame,
) -> Vec<format::Node<'a>> {
    let nodes = document.nodes().unwrap_or_default();
    let by_id: HashMap<&'a str, format::Node<'a>> =
        nodes.iter().map(|node| (node.id(), node)).collect();
    let mut container: HashMap<&'a str, &'a str> = HashMap::new();
    for node in nodes.iter() {
        let Some(group) = node.component_as_group() else {
            continue;
        };
        for child in group.child_ids().unwrap_or_default() {
            if child != node.id() && by_id.contains_key(child) {
                container.entry(child).or_insert(node.id());
            }
        }
    }

    let mut visited: HashSet<&'a str> = HashSet::new();
    let mut out = Vec::with_capacity(nodes.len());
    // Depth-first with an explicit stack; children are pushed in reverse so
    // the first child pops first.
    let roots: Vec<&'a str> = nodes
        .iter()
        .filter(|node| !container.contains_key(node.id()))
        .map(|node| node.id())
        .collect();
    let mut stack = sorted_siblings(roots, frame);
    stack.reverse();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        let Some(node) = by_id.get(id).copied() else {
            continue;
        };
        out.push(node);
        if let Some(group) = node.component_as_group() {
            let shown = solo_child(node.id(), group, frame);
            let mask = group.mask_child_id();
            let children: Vec<&'a str> = group
                .child_ids()
                .unwrap_or_default()
                .iter()
                .filter(|child| container.get(child) == Some(&id))
                .filter(|child| match shown {
                    Some(solo) => *child == solo || Some(*child) == mask,
                    None => true,
                })
                .collect();
            let children = sorted_siblings(children, frame);
            stack.extend(children.into_iter().rev());
        }
    }
    out
}

/// The child a group shows this frame, or `None` for every child.
///
/// A `soloIndex` override wins over the file's `solo_child_id`: it indexes
/// the group's `child_ids` as written, and -1 — or an index the group does not
/// have — shows every child (`soloChildAt` in the editor). The file's own
/// solo is taken as written, as `paintedChildren` takes it.
pub fn solo_child<'a>(
    group_id: &str,
    group: Group<'a>,
    frame: &EvaluatedFrame,
) -> Option<&'a str> {
    let children = group.child_ids().unwrap_or_default();
    let keyed = frame
        .node_overrides
        .get(group_id)
        .and_then(|values| values.scalar("soloIndex"));
    match keyed {
        Some(index) => {
            let index = index.round();
            if index >= 0.0 && (index as usize) < children.len() {
                Some(children.get(index as usize))
            } else {
                None
            }
        }
        None => group.solo_child_id(),
    }
}

/// [`ids`] — one sibling list, back to front — re-sorted by this frame's
/// `drawOrder` overrides: a keyed sibling sorts as its rounded value, an
/// unkeyed one as its own position, ties keep the list's order. The rule of
/// `withPosedStackOrder` in the editor. A list with no keyed sibling comes
/// back untouched.
pub fn sorted_siblings<'a>(ids: Vec<&'a str>, frame: &EvaluatedFrame) -> Vec<&'a str> {
    let keyed = |id: &str| {
        frame
            .node_overrides
            .get(id)
            .and_then(|values| values.scalar("drawOrder"))
    };
    if !ids.iter().any(|id| keyed(id).is_some()) {
        return ids;
    }
    let mut indexed: Vec<(usize, &'a str, f64)> = ids
        .into_iter()
        .enumerate()
        .map(|(i, id)| (i, id, keyed(id).map(f64::round).unwrap_or(i as f64)))
        .collect();
    indexed.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
    indexed.into_iter().map(|(_, id, _)| id).collect()
}

/// Clips each command to the mask of the nearest enclosing group that has one.
///
/// Runs as a second pass because a mask is a *sibling*: the command that
/// describes it may not exist yet when the commands it clips are built.
///
/// The rule matches `effectiveMaskChildId` in `document_helpers.dart:245`: a
/// mask counts only when it names a direct child and the group has something
/// else to clip. A group whose only child is its mask paints that child as
/// ordinary artwork instead of compositing itself away to nothing.
///
/// The mask node keeps drawing. That is what a creative-app clipping mask
/// means and what `frame_renderer.dart:474-483` does — the base stays visible
/// and its alpha clips the siblings above it.
fn attach_clip_masks(document: &format::Document<'_>, commands: &mut [RenderCommand]) {
    let groups: HashMap<&str, Group<'_>> = document
        .nodes()
        .unwrap_or_default()
        .iter()
        .filter_map(|node| node.component_as_group().map(|group| (node.id(), group)))
        .collect();
    let mut masked_by: HashMap<&str, &str> = HashMap::new();
    for node in document.nodes().unwrap_or_default() {
        let Some(group) = node.component_as_group() else {
            continue;
        };
        let Some(mask_id) = group.mask_child_id() else {
            continue;
        };
        let children = group.child_ids().unwrap_or_default();
        if children.len() < 2 || !children.iter().any(|child| child == mask_id) {
            continue;
        }
        // The mask does not clip itself. Everything else in the group is
        // clipped, however deep: artwork usually sits in its own group inside
        // the clip group (an iris inside the eye white's group), and a group
        // draws nothing of its own, so clipping only direct children left
        // nested artwork unmasked.
        let mut pending: Vec<&str> = children
            .iter()
            .filter(|child| *child != mask_id)
            .collect();
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            // A nested clip group's own mask is nearer, so it wins.
            if let Some(inner) = groups.get(id) {
                if inner.mask_child_id().is_none() {
                    pending.extend(inner.child_ids().unwrap_or_default().iter());
                }
                continue;
            }
            masked_by.insert(id, mask_id);
        }
    }
    if masked_by.is_empty() {
        return;
    }

    // A mask can only clip with a shape, so a group used as one produces no
    // clip rather than an empty one that would erase everything.
    let clips: HashMap<&str, RenderClip> = commands
        .iter()
        // Text has no assembled outline to clip with, so a text node named as
        // a mask produces no clip — the content draws unmasked rather than
        // being erased.
        .filter(|command| !matches!(command.shape, RenderShape::Text { .. }))
        .filter(|command| masked_by.values().any(|id| *id == command.node_id))
        .map(|command| {
            (
                command.node_id.as_str(),
                RenderClip {
                    node_id: command.node_id.clone(),
                    shape: command.shape.clone(),
                    parent: command.parent,
                    transform: command.transform,
                },
            )
        })
        .collect::<HashMap<_, _>>()
        .into_iter()
        .collect();

    let resolved: Vec<Option<RenderClip>> = commands
        .iter()
        .map(|command| {
            masked_by
                .get(command.node_id.as_str())
                .and_then(|mask_id| clips.get(mask_id))
                .cloned()
        })
        .collect();
    for (command, clip) in commands.iter_mut().zip(resolved) {
        command.clip = clip;
    }
}

/// The transform that fits a document of `canvas` size into a `surface` of
/// pixels, and the offset that centres it.
///
/// The policy is *contain*: scale uniformly by the tighter of the two axes and
/// centre the remainder, so the whole document is visible and nothing is
/// stretched. Uniform is the important half — a non-uniform fit would silently
/// distort every circle in the document.
///
/// Returns identity when either size is degenerate, so a surface that has not
/// been laid out yet draws 1:1 rather than collapsing to nothing.
pub fn fit_transform(canvas: (f64, f64), surface: (f64, f64)) -> Affine {
    let (cw, ch) = canvas;
    let (sw, sh) = surface;
    if !(cw > 0.0 && ch > 0.0 && sw > 0.0 && sh > 0.0) {
        return IDENTITY;
    }
    let scale = (sw / cw).min(sh / ch);
    [
        scale,
        0.0,
        0.0,
        scale,
        (sw - cw * scale) / 2.0,
        (sh - ch * scale) / 2.0,
    ]
}

/// Whether the runtime draws a component, and if not, why.
///
/// FlatBuffers emits schema enums as newtype structs, so Rust cannot force the
/// dispatch above to be exhaustive the way Dart forces the editor-side ledger
/// (`lib/src/data/runtime_capabilities.dart`). This function is the stand-in:
/// `tests/schema_coverage.rs` walks `ENUM_VALUES_COMPONENT` — which the
/// generator regenerates from the schema — and fails on any component that
/// lands in [`ComponentSupport::Unclassified`]. Adding a component to the
/// schema without deciding what the runtime does with it breaks that test.
pub fn component_support(component: Component) -> ComponentSupport {
    match component {
        Component::Shape | Component::Path | Component::CompoundPath => ComponentSupport::Drawn,
        Component::Group => ComponentSupport::NotDrawn(
            "a group is a transform and a compositing scope; its children draw",
        ),
        Component::NONE => ComponentSupport::NotDrawn("no component"),
        Component::Text => ComponentSupport::Drawn,
        Component::Image => ComponentSupport::Drawn,
        _ => ComponentSupport::Unclassified,
    }
}

/// The verdict [`component_support`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentSupport {
    /// The renderer emits a command for it.
    Drawn,
    /// Deliberately not drawn, for the stated reason.
    NotDrawn(&'static str),
    /// In the schema, and nobody has said what the runtime does with it.
    Unclassified,
}

/// Lays a text node out into positioned glyphs.
///
/// Layout is deliberately simple: one advance after another along each line,
/// with the node's `letter_spacing` and `line_height`, and the line aligned in
/// the node's own box. That covers Latin text in the families the editor
/// bundles. It is *not* full shaping — no kerning pairs from GPOS, no
/// ligatures, no bidi or complex scripts — so text that needs those sits a
/// little differently here than in the editor. That is a real divergence and
/// it is recorded on the capability ledger rather than left to be discovered
/// on a device.
///
/// Text with no font in the document draws nothing: the runtime has no
/// system-font lookup on purpose, because substituting a face silently
/// renders the document in the wrong one.
fn text_command(
    document: &format::Document<'_>,
    node: format::Node<'_>,
    text: format::generated::kineto::Text<'_>,
    opacity: f64,
    parent: Affine,
    transform: RenderTransform,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Option<RenderCommand> {
    use skrifa::{prelude::*, MetadataProvider};

    let content = text.content()?;
    if content.is_empty() {
        return None;
    }

    let family = text.font_family().unwrap_or_default();
    let font_data = font_bytes(document, family, text.font_weight(), text.italic())?;
    let font = FontRef::new(&font_data).ok()?;

    // `fontSize`, `letterSpacing` and `lineHeight` are animation-only in the
    // editor — the document model has no static field for the latter two — so
    // the track *is* the value. Reading them here is what makes those three
    // properties mean anything at runtime.
    let size = overrides
        .and_then(|values| values.scalar("fontSize"))
        .unwrap_or(text.font_size() as f64);
    let charmap = font.charmap();
    let metrics = font.metrics(Size::new(size as f32), LocationRef::default());
    let glyph_metrics = font.glyph_metrics(Size::new(size as f32), LocationRef::default());

    // The editor treats `lineHeight` as a multiplier of the font size; a node
    // with no track gets the face's own line spacing.
    let line_height = overrides
        .and_then(|values| values.scalar("lineHeight"))
        .map(|multiplier| multiplier * size)
        .unwrap_or((metrics.ascent - metrics.descent + metrics.leading) as f64);
    let letter_spacing = overrides
        .and_then(|values| values.scalar("letterSpacing"))
        .unwrap_or(0.0);

    let mut glyphs = Vec::new();
    let mut baseline = metrics.ascent as f64;
    for line in content.split('\n') {
        let mut positioned = Vec::new();
        let mut pen = 0.0;
        for ch in line.chars() {
            let Some(id) = charmap.map(ch) else { continue };
            positioned.push(RenderGlyph {
                id: id.to_u32(),
                x: pen,
                y: baseline,
            });
            pen += glyph_metrics.advance_width(id).unwrap_or(0.0) as f64 + letter_spacing;
        }

        // Alignment is measured against the node's authored box, which is what
        // the editor draws into.
        let offset = match text.align() {
            format::generated::kineto::TextAlign::Center => (text.width() as f64 - pen) / 2.0,
            format::generated::kineto::TextAlign::Right => text.width() as f64 - pen,
            // Justify has no runtime implementation and falls back to left
            // rather than silently centring.
            _ => 0.0,
        };
        for glyph in &mut positioned {
            glyph.x += offset;
        }
        glyphs.extend(positioned);
        baseline += line_height;
    }

    if glyphs.is_empty() {
        return None;
    }

    let colour = text
        .color()
        .and_then(|colour| color_from_kineto(colour, opacity))
        .unwrap_or(RenderColor {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        });

    Some(RenderCommand {
        clip: None,
        shadows: Vec::new(),
        blend_mode: blend_mode_from_kineto(node.blend_mode()).unwrap_or_default(),
        parent,
        node_id: node.id().to_owned(),
        node_name: node.name().to_owned(),
        opacity,
        transform,
        shape: RenderShape::Text {
            font_data,
            font_size: size,
            glyphs,
        },
        fills: vec![RenderPaint::Solid(colour)],
        stroke: None,
    })
}

/// The bytes for the closest face the document ships.
///
/// Weight and italic are matched exactly first; failing that, any face in the
/// same family, because drawing the text in the wrong weight is better than
/// not drawing it. A family the document does not carry returns nothing.
fn font_bytes(
    document: &format::Document<'_>,
    family: &str,
    weight: u16,
    italic: bool,
) -> Option<Vec<u8>> {
    let fonts = document.fonts()?;
    let mut fallback = None;
    for face in fonts.iter() {
        if face.family() != family {
            continue;
        }
        let data = face.data()?;
        if face.weight() == weight && face.italic() == italic {
            return Some(data.bytes().to_vec());
        }
        if fallback.is_none() {
            fallback = Some(data.bytes().to_vec());
        }
    }
    fallback
}

/// Replaces a path's vertices with an evaluated `pathData` morph.
///
/// A vertex-count mismatch holds the authored geometry, mirroring the editor
/// (`frame_renderer.dart:685`): a morph recorded against a different shape has
/// no correspondence to apply, and stretching one onto the other would produce
/// a shape nobody drew.
///
/// The morph carries anchors and handles only. Bone bindings and weights stay
/// with the authored vertex, because a morph is about where the outline is,
/// not what drives it.
fn apply_morph(
    points: Vec<RuntimePathPoint>,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Vec<RuntimePathPoint> {
    let Some(morph) = overrides.and_then(|values| values.path("pathData")) else {
        return points;
    };
    if morph.len() != points.len() {
        return points;
    }
    points
        .into_iter()
        .zip(morph)
        .map(|(point, vertex)| RuntimePathPoint {
            anchor: crate::bones::Point2 {
                x: vertex.anchor.0,
                y: vertex.anchor.1,
            },
            in_handle: crate::bones::Point2 {
                x: vertex.in_handle.0,
                y: vertex.in_handle.1,
            },
            out_handle: crate::bones::Point2 {
                x: vertex.out_handle.0,
                y: vertex.out_handle.1,
            },
            ..point
        })
        .collect()
}

/// The drop shadows an effect list carries.
///
/// Inset shadows are skipped: they need the shape's interior masked, which the
/// flat command model has nowhere to express, and drawing them as ordinary
/// shadows would put the darkening on the wrong side.
fn shadows_from_effects(
    effects: Option<
        flatbuffers::Vector<
            '_,
            flatbuffers::ForwardsUOffset<format::generated::kineto::EffectEntry<'_>>,
        >,
    >,
    opacity: f64,
) -> Vec<RenderShadow> {
    let Some(effects) = effects else {
        return Vec::new();
    };
    effects
        .iter()
        .filter_map(|entry| {
            let shadow = entry.kind_as_drop_shadow()?;
            if !shadow.enabled() || shadow.inset() {
                return None;
            }
            Some(RenderShadow {
                color: color_from_kineto(shadow.color()?, opacity * shadow.opacity() as f64)?,
                offset_x: shadow.offset_x() as f64,
                offset_y: shadow.offset_y() as f64,
                blur: shadow.blur() as f64,
                spread: shadow.spread() as f64,
            })
        })
        .collect()
}

/// Decodes an image node's PNG and places it in the scene.
///
/// A PNG the decoder rejects produces no command rather than a blank
/// rectangle: an image that fails to decode is a broken asset, and drawing a
/// placeholder over the artwork hides that.
fn image_command(
    node: format::Node<'_>,
    image: format::generated::kineto::Image<'_>,
    opacity: f64,
    parent: Affine,
    transform: RenderTransform,
) -> Option<RenderCommand> {
    let bytes = image.png_data()?;
    let (rgba, pixel_width, pixel_height) = decode_png(bytes.bytes())?;

    Some(RenderCommand {
        clip: None,
        shadows: Vec::new(),
        blend_mode: blend_mode_from_kineto(node.blend_mode()).unwrap_or_default(),
        parent,
        node_id: node.id().to_owned(),
        node_name: node.name().to_owned(),
        opacity,
        transform,
        shape: RenderShape::Image {
            width: image.width() as f64,
            height: image.height() as f64,
            rgba,
            pixel_width,
            pixel_height,
        },
        fills: Vec::new(),
        stroke: None,
    })
}

/// Decodes a PNG to premultiplied RGBA8, which is what vello's image brush
/// wants.
fn decode_png(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());

    let rgba: Vec<u8> = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => buffer,
        (png::ColorType::Rgb, png::BitDepth::Eight) => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        (png::ColorType::Grayscale, png::BitDepth::Eight) => {
            buffer.iter().flat_map(|v| [*v, *v, *v, 255]).collect()
        }
        (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => buffer
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        // Anything else (16-bit, a palette the transformer did not expand) is
        // left undrawn rather than guessed at.
        _ => return None,
    };

    // Premultiply: vello composites premultiplied, and straight alpha shows as
    // a bright fringe around every soft edge.
    let rgba = rgba
        .chunks_exact(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            [
                ((p[0] as u32 * a) / 255) as u8,
                ((p[1] as u32 * a) / 255) as u8,
                ((p[2] as u32 * a) / 255) as u8,
                p[3],
            ]
        })
        .collect();

    Some((rgba, info.width, info.height))
}

fn shape_command(
    node: format::Node<'_>,
    shape: format::generated::kineto::Shape<'_>,
    opacity: f64,
    parent: Affine,
    transform: RenderTransform,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Option<RenderCommand> {
    let render_shape = if let Some(rect) = shape.geometry_as_rectangle() {
        RenderShape::Rectangle {
            width: overrides
                .and_then(|values| values.scalar("width"))
                .unwrap_or(rect.width() as f64),
            height: overrides
                .and_then(|values| values.scalar("height"))
                .unwrap_or(rect.height() as f64),
            corner_radius: overrides
                .and_then(|values| values.scalar("cornerRadius"))
                .unwrap_or(rect.corner_radius() as f64)
                .max(0.0),
        }
    } else if let Some(ellipse) = shape.geometry_as_ellipse() {
        RenderShape::Ellipse {
            width: overrides
                .and_then(|values| values.scalar("width"))
                .unwrap_or(ellipse.width() as f64),
            height: overrides
                .and_then(|values| values.scalar("height"))
                .unwrap_or(ellipse.height() as f64),
        }
    } else {
        return None;
    };

    Some(RenderCommand {
        // Filled in by attach_clip_masks, which needs every command first.
        clip: None,
        shadows: shadows_from_effects(shape.effects(), opacity),
        blend_mode: blend_mode_from_kineto(node.blend_mode()).unwrap_or_default(),
        parent,
        node_id: node.id().to_owned(),
        node_name: node.name().to_owned(),
        opacity,
        transform,
        shape: render_shape,
        fills: shape_fill(shape, opacity, overrides),
        stroke: shape_stroke(shape.stroke(), opacity, overrides),
    })
}

fn path_command(
    node: format::Node<'_>,
    path: format::generated::kineto::Path<'_>,
    opacity: f64,
    parent: Affine,
    transform: RenderTransform,
    overrides: Option<&crate::animation::NodeOverrides>,
    current_fk: &std::collections::HashMap<String, crate::bones::Matrix4>,
    rest_fk: &std::collections::HashMap<String, crate::bones::Matrix4>,
) -> Option<RenderCommand> {
    let points = path
        .nodes_v2()
        .map(|nodes| {
            nodes
                .iter()
                .map(RuntimePathPoint::from_path_node_v2)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if points.is_empty() {
        return None;
    }

    // A `pathData` morph replaces the vertices before the skeleton deforms
    // them: the morph is the shape the author drew, and skinning moves that
    // shape. Doing it the other way would deform the rest pose and then throw
    // the result away.
    let points = apply_morph(points, overrides);

    let (local_to_world, world_to_local) = skinning_space(parent, transform);
    let points = apply_skeletal_transforms(
        &points,
        current_fk,
        rest_fk,
        local_to_world,
        world_to_local,
    );
    Some(RenderCommand {
        // Filled in by attach_clip_masks, which needs every command first.
        clip: None,
        shadows: shadows_from_effects(path.effects(), opacity),
        blend_mode: blend_mode_from_kineto(node.blend_mode()).unwrap_or_default(),
        parent,
        node_id: node.id().to_owned(),
        node_name: node.name().to_owned(),
        opacity,
        transform,
        shape: RenderShape::Path {
            // A single contour cannot self-overlap in a way the two rules
            // disagree about, so non-zero is the honest default here.
            even_odd: false,
            contours: vec![RenderPathContour {
                closed: path.closed(),
                points: points
                    .into_iter()
                    .map(RenderPathPoint::from_runtime)
                    .collect(),
            }],
        },
        fills: path_fill(path, opacity, overrides),
        stroke: shape_stroke(path.stroke(), opacity, overrides),
    })
}

fn compound_path_command(
    node: format::Node<'_>,
    path: format::generated::kineto::CompoundPath<'_>,
    opacity: f64,
    parent: Affine,
    transform: RenderTransform,
    overrides: Option<&crate::animation::NodeOverrides>,
    current_fk: &std::collections::HashMap<String, crate::bones::Matrix4>,
    rest_fk: &std::collections::HashMap<String, crate::bones::Matrix4>,
) -> Option<RenderCommand> {
    let (local_to_world, world_to_local) = skinning_space(parent, transform);
    let contours = path
        .contours()?
        .iter()
        .filter_map(|contour| {
            let points = contour.nodes_v2()?;
            let points = points
                .iter()
                .map(RuntimePathPoint::from_path_node_v2)
                .collect::<Vec<_>>();
            let points = apply_skeletal_transforms(
                &points,
                current_fk,
                rest_fk,
                local_to_world,
                world_to_local,
            );
            Some(RenderPathContour {
                closed: contour.closed(),
                points: points
                    .into_iter()
                    .map(RenderPathPoint::from_runtime)
                    .collect(),
            })
        })
        .collect::<Vec<_>>();

    if contours.is_empty() {
        return None;
    }

    Some(RenderCommand {
        // Filled in by attach_clip_masks, which needs every command first.
        clip: None,
        // CompoundPath has no effects member in the schema, so a compound
        // path with a shadow has nowhere to carry it.
        shadows: Vec::new(),
        blend_mode: blend_mode_from_kineto(node.blend_mode()).unwrap_or_default(),
        parent,
        node_id: node.id().to_owned(),
        node_name: node.name().to_owned(),
        opacity,
        transform,
        shape: RenderShape::Path {
            contours,
            even_odd: path.fill_rule() == format::generated::kineto::PathFillRule::EvenOdd,
        },
        fills: compound_path_fill(path, opacity, overrides),
        stroke: shape_stroke(path.stroke(), opacity, overrides),
    })
}

/// The opacity every ancestor contributes, or `None` when one of them is
/// hidden.
///
/// Multiplying is not the same as the editor's per-group layer — overlapping
/// fill and stroke inside a faded group composite differently — but it is the
/// difference between "fades" and "does not fade at all", which is what this
/// closes.
fn inherited_opacity(
    node_id: &str,
    parents: &HashMap<String, String>,
    nodes: &HashMap<&str, format::Node<'_>>,
    frame: &EvaluatedFrame,
) -> Option<f64> {
    let mut opacity = 1.0;
    let mut current = parents.get(node_id);
    let mut guard = 0;
    while let Some(parent_id) = current {
        if guard > nodes.len() {
            // A cycle in a hand-written file stops the walk rather than
            // hanging the runtime, matching `world_affines`.
            break;
        }
        guard += 1;
        if let Some(parent) = nodes.get(parent_id.as_str()) {
            if !parent.visible() {
                return None;
            }
            let parent_opacity = frame
                .node_overrides
                .get(parent_id.as_str())
                .and_then(|values| values.scalar("opacity"))
                .unwrap_or(parent.opacity() as f64)
                .clamp(0.0, 1.0);
            opacity *= parent_opacity;
        }
        current = parents.get(parent_id);
    }
    Some(opacity)
}

fn render_transform(
    transform: Option<format::Transform<'_>>,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> RenderTransform {
    let base = transform
        .map(|transform| RenderTransform {
            x: transform.x() as f64,
            y: transform.y() as f64,
            rotation: transform.rotation() as f64,
            scale_x: transform.scale_x() as f64,
            scale_y: transform.scale_y() as f64,
            anchor_x: transform.anchor_x() as f64,
            anchor_y: transform.anchor_y() as f64,
        })
        .unwrap_or_default();

    RenderTransform {
        x: overrides
            .and_then(|values| values.scalar("positionX"))
            .unwrap_or(base.x),
        y: overrides
            .and_then(|values| values.scalar("positionY"))
            .unwrap_or(base.y),
        rotation: overrides
            .and_then(|values| values.scalar("rotation"))
            .unwrap_or(base.rotation),
        scale_x: overrides
            .and_then(|values| values.scalar("scaleX"))
            .unwrap_or(base.scale_x),
        scale_y: overrides
            .and_then(|values| values.scalar("scaleY"))
            .unwrap_or(base.scale_y),
        anchor_x: overrides
            .and_then(|values| values.scalar("anchorX"))
            .unwrap_or(base.anchor_x),
        anchor_y: overrides
            .and_then(|values| values.scalar("anchorY"))
            .unwrap_or(base.anchor_y),
    }
}

fn shape_fill(
    shape: format::generated::kineto::Shape<'_>,
    opacity: f64,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Vec<RenderPaint> {
    let mut fills = all_fills_v2(shape.fills_v2(), opacity, overrides);
    if fills.is_empty() {
        // The v1 single-fill fields, for files written before fills_v2.
        fills.extend(
            fill_v2_paint(shape.fill_v2(), opacity, overrides)
                .or_else(|| fill_color(shape.fill(), opacity)),
        );
    }
    let override_color = overrides.and_then(|values| values.color("fillColor"));
    fills
        .into_iter()
        .map(|fill| override_fill(fill, override_color))
        .collect()
}

fn path_fill(
    path: format::generated::kineto::Path<'_>,
    opacity: f64,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Vec<RenderPaint> {
    let mut fills = all_fills_v2(path.fills_v2(), opacity, overrides);
    if fills.is_empty() {
        // The v1 single-fill fields, for files written before fills_v2.
        fills.extend(
            fill_v2_paint(path.fill_v2(), opacity, overrides)
                .or_else(|| fill_color(path.fill(), opacity)),
        );
    }
    let override_color = overrides.and_then(|values| values.color("fillColor"));
    fills
        .into_iter()
        .map(|fill| override_fill(fill, override_color))
        .collect()
}

fn compound_path_fill(
    path: format::generated::kineto::CompoundPath<'_>,
    opacity: f64,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Vec<RenderPaint> {
    let mut fills = all_fills_v2(path.fills_v2(), opacity, overrides);
    if fills.is_empty() {
        // The v1 single-fill fields, for files written before fills_v2.
        fills.extend(
            fill_v2_paint(path.fill_v2(), opacity, overrides)
                .or_else(|| fill_color(path.fill(), opacity)),
        );
    }
    let override_color = overrides.and_then(|values| values.color("fillColor"));
    fills
        .into_iter()
        .map(|fill| override_fill(fill, override_color))
        .collect()
}

fn all_fills_v2(
    fills: Option<
        flatbuffers::Vector<
            '_,
            flatbuffers::ForwardsUOffset<format::generated::kineto::FillV2<'_>>,
        >,
    >,
    opacity: f64,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Vec<RenderPaint> {
    let Some(fills) = fills else {
        return Vec::new();
    };
    fills
        .iter()
        .filter_map(|fill| fill_v2_paint(Some(fill), opacity, overrides))
        .collect()
}

/// Reads a `FillV2` union, gradients included.
///
/// This used to be `kind_as_solid_fill()?`, so a gradient node returned `None`
/// and drew with no fill at all — the single biggest "it looks wrong" for a
/// designed document (issue #111 parity audit §3).
fn fill_v2_paint(
    fill: Option<format::generated::kineto::FillV2<'_>>,
    opacity: f64,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Option<RenderPaint> {
    let fill = fill?;

    if let Some(solid) = fill.kind_as_solid_fill() {
        return color_from_kineto(solid.color()?, opacity * solid.opacity() as f64)
            .map(RenderPaint::Solid);
    }

    if let Some(linear) = fill.kind_as_linear_gradient_fill() {
        let stops = gradient_stops(linear.stops(), opacity, overrides);
        if stops.is_empty() {
            return None;
        }
        return Some(RenderPaint::LinearGradient {
            stops,
            from: (linear.p0x() as f64, linear.p0y() as f64),
            to: (linear.p1x() as f64, linear.p1y() as f64),
            spread: spread_from_kineto(linear.spread()),
        });
    }

    if let Some(radial) = fill.kind_as_radial_gradient_fill() {
        let stops = gradient_stops(radial.stops(), opacity, overrides);
        if stops.is_empty() {
            return None;
        }
        return Some(RenderPaint::RadialGradient {
            stops,
            center: (radial.cx() as f64, radial.cy() as f64),
            focal: (radial.fx() as f64, radial.fy() as f64),
            radius: radial.r() as f64,
            spread: spread_from_kineto(radial.spread()),
        });
    }

    None
}

/// Reads a gradient's stops, letting the animation move them.
///
/// `gradientStopColor`, `gradientStopOpacity` and `gradientStopOffset` used to
/// export as `FillColor`/`Opacity` — the "closest analogue" — so animating a
/// stop replaced the node's whole fill or faded the node (issue #111). They
/// have kinds of their own now, and drive every stop of the gradient
/// together: the schema keys an override by node, not by stop index, so
/// per-stop animation is still out of reach and is recorded as such.
fn gradient_stops(
    stops: Option<
        flatbuffers::Vector<
            '_,
            flatbuffers::ForwardsUOffset<format::generated::kineto::GradientStop<'_>>,
        >,
    >,
    opacity: f64,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Vec<RenderGradientStop> {
    let Some(stops) = stops else {
        return Vec::new();
    };
    let colour_override = overrides.and_then(|values| values.color("gradientStopColor"));
    let opacity_override = overrides.and_then(|values| values.scalar("gradientStopOpacity"));
    let offset_override = overrides.and_then(|values| values.scalar("gradientStopOffset"));

    stops
        .iter()
        .filter_map(|stop| {
            let alpha = opacity * opacity_override.unwrap_or(1.0);
            let colour = color_from_kineto(stop.color()?, alpha)?;
            Some(RenderGradientStop {
                // The offset track shifts every stop by the same amount rather
                // than snapping them all together, which would collapse the
                // gradient to one colour.
                offset: (stop.offset() as f64 + offset_override.unwrap_or(0.0)).clamp(0.0, 1.0),
                color: override_color(colour, colour_override),
            })
        })
        .collect()
}

fn spread_from_kineto(spread: format::generated::kineto::SpreadMethod) -> RenderSpread {
    match spread {
        format::generated::kineto::SpreadMethod::Reflect => RenderSpread::Reflect,
        format::generated::kineto::SpreadMethod::Repeat => RenderSpread::Repeat,
        // Pad is both the schema default and the sane answer for a value this
        // build does not know.
        _ => RenderSpread::Pad,
    }
}

fn fill_color(
    fill: Option<format::generated::kineto::Fill<'_>>,
    opacity: f64,
) -> Option<RenderPaint> {
    let fill = fill?;
    color_from_kineto(fill.color()?, opacity * fill.opacity() as f64).map(RenderPaint::Solid)
}

fn shape_stroke(
    stroke: Option<format::generated::kineto::Stroke<'_>>,
    opacity: f64,
    overrides: Option<&crate::animation::NodeOverrides>,
) -> Option<RenderStroke> {
    let stroke = stroke?;
    // A stroke still paints one colour: the schema's stroke fill can hold a
    // gradient, but `RenderStroke` has nowhere to put one yet, so a gradient
    // stroke collapses to its first stop rather than vanishing.
    let color = fill_v2_paint(stroke.stroke_fill_v2(), opacity * stroke.opacity() as f64, overrides)
        .map(|paint| paint.approximate_color())
        .or_else(|| color_from_kineto(stroke.color()?, opacity * stroke.opacity() as f64))?;
    let color = override_color(
        color,
        overrides.and_then(|values| values.color("strokeColor")),
    );
    Some(RenderStroke {
        color,
        width: overrides
            .and_then(|values| values.scalar("strokeWidth"))
            .unwrap_or(stroke.width() as f64),
        cap: match stroke.cap() {
            format::generated::kineto::StrokeCap::Round => RenderCap::Round,
            format::generated::kineto::StrokeCap::Square => RenderCap::Square,
            // Butt is the schema default and the right answer for a value this
            // build does not know.
            _ => RenderCap::Butt,
        },
        join: match stroke.join() {
            format::generated::kineto::StrokeJoin::Round => RenderJoin::Round,
            format::generated::kineto::StrokeJoin::Bevel => RenderJoin::Bevel,
            _ => RenderJoin::Miter,
        },
        miter_limit: stroke.miter_limit() as f64,
        dash_array: stroke
            .dash_array()
            .map(|dashes| dashes.iter().map(|d| d as f64).collect())
            .unwrap_or_default(),
        dash_offset: stroke.dash_offset() as f64,
        align: match stroke.align() {
            format::generated::kineto::StrokeAlign::Inside => RenderStrokeAlign::Inside,
            format::generated::kineto::StrokeAlign::Outside => RenderStrokeAlign::Outside,
            // Center is the schema default and the only safe answer for a
            // value this build does not know.
            _ => RenderStrokeAlign::Center,
        },
        trim_start: overrides
            .and_then(|values| values.scalar("strokeTrimStart"))
            .unwrap_or(stroke.trim_start() as f64),
        trim_end: overrides
            .and_then(|values| values.scalar("strokeTrimEnd"))
            .unwrap_or(stroke.trim_end() as f64),
        trim_offset: overrides
            .and_then(|values| values.scalar("strokeTrimOffset"))
            .unwrap_or(stroke.trim_offset() as f64),
    })
}

fn color_from_kineto(color: &format::Color, opacity: f64) -> Option<RenderColor> {
    let alpha = (color.a() as f64 * opacity.clamp(0.0, 1.0)).round() as u8;
    Some(RenderColor {
        r: color.r(),
        g: color.g(),
        b: color.b(),
        a: alpha,
    })
}

/// Applies a `fillColor` animation track to a paint.
///
/// A track only replaces a *solid* fill. Animating one colour over a gradient
/// would have to mean recolouring every stop, which is the editor's separate
/// `gradientStopColor` property — and that one has no runtime representation
/// and is reported as such by the export pre-flight. Silently flattening the
/// gradient to a solid here would be the same class of mistake this issue
/// exists to remove.
fn override_fill(base: RenderPaint, color: Option<u32>) -> RenderPaint {
    match base {
        RenderPaint::Solid(solid) => RenderPaint::Solid(override_color(solid, color)),
        gradient => gradient,
    }
}

fn override_color(base: RenderColor, color: Option<u32>) -> RenderColor {
    let Some(color) = color else {
        return base;
    };
    let [a, r, g, b] = color.to_be_bytes();
    let effective_opacity = base.a as f64 / 255.0;
    RenderColor {
        r,
        g,
        b,
        a: (a as f64 * effective_opacity).round() as u8,
    }
}

impl RenderPathPoint {
    /// Public so the constraint solver can measure a path's curve bounds
    /// with the same decomposition the renderer draws with.
    pub fn from_runtime(point: RuntimePathPoint) -> Self {
        Self {
            anchor_x: point.anchor.x,
            anchor_y: point.anchor.y,
            in_handle_x: point.in_handle.x,
            in_handle_y: point.in_handle.y,
            out_handle_x: point.out_handle.x,
            out_handle_y: point.out_handle.y,
        }
    }
}

#[cfg(feature = "vello-render")]
pub fn build_vello_scene(render_scene: &RenderScene) -> vello::Scene {
    build_vello_scene_fitted(render_scene, None)
}

/// Builds the vello scene, fitting the document into `surface` when one is
/// given.
///
/// `None` draws 1:1 in document units, which is what every caller got before
/// and what the unit tests want. A surface fits the document into it with
/// [`fit_transform`].
#[cfg(feature = "vello-render")]
pub fn build_vello_scene_fitted(
    render_scene: &RenderScene,
    surface: Option<(f64, f64)>,
) -> vello::Scene {
    let mut scene = vello::Scene::new();
    let fit = match surface {
        Some(surface) => fit_transform(render_scene.canvas, surface),
        None => IDENTITY,
    };
    // The camera sits inside the fit: it moves the document within its own
    // canvas, and the fit places that canvas on the drawable.
    let fit = crate::hierarchy::multiply(fit, render_scene.camera);
    for command in &render_scene.commands {
        draw_command(&mut scene, command, fit);
    }
    scene
}

#[cfg(feature = "vello-render")]
fn draw_command(scene: &mut vello::Scene, command: &RenderCommand, fit: Affine) {
    use vello::kurbo::{Ellipse, Rect, Shape as _};
    use vello::peniko::Fill;

    // A clipped command draws inside a layer bounded by the mask shape.
    //
    // This is a geometric clip, where the editor composites with `dstIn`
    // against the mask's alpha (`frame_renderer.dart:479`). For an opaque mask
    // — which is what a clipping mask normally is — the two agree exactly. A
    // semi-transparent or gradient-filled mask fades its content in the editor
    // and clips hard here; that difference is real, and it is recorded in the
    // capability ledger rather than left to be discovered.
    // A non-Normal blend needs its own layer: vello composites a layer against
    // the backdrop, where a bare fill just paints over it.
    let blended = if command.blend_mode != RenderBlendMode::Normal {
        scene.push_layer(
            Fill::NonZero,
            vello_mix(command.blend_mode),
            1.0,
            vello_parent(fit),
            &vello::kurbo::Rect::new(-1e6, -1e6, 1e6, 1e6),
        );
        true
    } else {
        false
    };

    let clipped = if let Some(clip) = &command.clip {
        let clip_transform =
            vello_parent(fit) * vello_parent(clip.parent) * vello_transform(clip.transform);
        scene.push_layer(
            Fill::NonZero,
            vello::peniko::Mix::Normal,
            1.0,
            clip_transform,
            &shape_path(&clip.shape),
        );
        true
    } else {
        false
    };

    let transform =
        vello_parent(fit) * vello_parent(command.parent) * vello_transform(command.transform);

    // Shadows go down before the shape, so the shape sits on top of them.
    //
    // Drawn as a blurred rounded rect over the shape's bounds rather than the
    // shape's own outline. That is not a shortcut: it is what the editor does
    // (`frame_renderer.dart:653-655` blurs `bounds.inflate(blur)`), and the
    // two renderers agreeing matters more here than either being prettier.
    if !command.shadows.is_empty() {
        let bounds = shape_path(&command.shape).bounding_box();
        for shadow in &command.shadows {
            let spread = shadow.spread;
            scene.draw_blurred_rounded_rect(
                transform
                    * vello::kurbo::Affine::translate((shadow.offset_x, shadow.offset_y)),
                bounds.inflate(spread, spread),
                vello_color(shadow.color),
                0.0,
                shadow.blur / 2.0,
            );
        }
    }
    match &command.shape {
        RenderShape::Rectangle {
            width,
            height,
            corner_radius,
        } => {
            let rect = Rect::new(0.0, 0.0, *width, *height);
            // A radius cannot exceed half the shorter side, or the corners
            // cross over each other.
            let radius = corner_radius.min(width.min(*height) / 2.0).max(0.0);
            // `Shape` is not dyn-compatible, so the rounded and square cases
            // are drawn on their own paths rather than through one box.
            if radius > 0.0 {
                let rounded = rect.to_rounded_rect(radius);
                for fill in &command.fills {
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        &vello_brush(fill, rounded.bounding_box()),
                        None,
                        &rounded,
                    );
                }
            } else {
                for fill in &command.fills {
                    scene.fill(
                        Fill::NonZero,
                        transform,
                        &vello_brush(fill, rect.bounding_box()),
                        None,
                        &rect,
                    );
                }
            }
        }
        RenderShape::Ellipse { width, height } => {
            let ellipse = Ellipse::from_rect(Rect::new(0.0, 0.0, *width, *height));
            for fill in &command.fills {
                scene.fill(
                        Fill::NonZero,
                        transform,
                        &vello_brush(fill, ellipse.bounding_box()),
                        None,
                        &ellipse,
                    );
            }
        }
        RenderShape::Text {
            font_data,
            font_size,
            glyphs,
        } => {
            let brush = command
                .fills
                .first()
                .map(|paint| vello_brush(paint, vello::kurbo::Rect::ZERO))
                .unwrap_or_else(|| {
                    vello::peniko::Brush::Solid(vello::peniko::Color::WHITE)
                });
            let font = vello::peniko::FontData::new(
                vello::peniko::Blob::from(font_data.clone()),
                0,
            );
            scene
                .draw_glyphs(&font)
                .font_size(*font_size as f32)
                .transform(transform)
                .brush(&brush)
                .draw(
                    Fill::NonZero,
                    glyphs.iter().map(|glyph| vello::Glyph {
                        id: glyph.id,
                        x: glyph.x as f32,
                        y: glyph.y as f32,
                    }),
                );
        }
        RenderShape::Image {
            width,
            height,
            rgba,
            pixel_width,
            pixel_height,
        } => {
            if let Some(image) = vello_image(rgba, *pixel_width, *pixel_height) {
                // The brush samples in its own pixel space, so the scale that
                // fits it into the node's `0..width x 0..height` box rides on
                // the brush transform rather than the scene transform.
                let scale = vello::kurbo::Affine::scale_non_uniform(
                    width / (*pixel_width).max(1) as f64,
                    height / (*pixel_height).max(1) as f64,
                );
                scene.draw_image(image.as_ref(), transform * scale);
            }
        }
        RenderShape::Path { contours, even_odd } => {
            let path = vello_path(contours);
            if path.area() == 0.0 && command.stroke.is_none() {
                return;
            }
            // The authored rule, not a fixed one: an even-odd compound path
            // is how a hole is drawn, and filling it non-zero fills the hole.
            let rule = if *even_odd {
                Fill::EvenOdd
            } else {
                Fill::NonZero
            };
            for fill in &command.fills {
                scene.fill(
                    rule,
                    transform,
                    &vello_brush(fill, path.bounding_box()),
                    None,
                    &path,
                );
            }
        }
    }

    if let Some(stroke) = &command.stroke {
        stroke_aligned(scene, stroke, transform, &shape_path(&command.shape));
    }

    if clipped {
        scene.pop_layer();
    }
    if blended {
        scene.pop_layer();
    }
}

/// Strokes `outline`, on the side the author asked for.
///
/// vello strokes centred on a path, so inside and outside are drawn as a
/// double-width centred stroke clipped to one half. That is how the shape's
/// own outline stays exactly where it is — offsetting the curve instead would
/// move it, and round-join artefacts on tight corners would differ from the
/// editor. `RenderStroke` used to have no alignment at all, so every stroke
/// straddled the outline however it was authored (issue #111).
#[cfg(feature = "vello-render")]
fn stroke_aligned(
    scene: &mut vello::Scene,
    stroke: &RenderStroke,
    transform: vello::kurbo::Affine,
    outline: &vello::kurbo::BezPath,
) {
    use vello::kurbo::Shape as _;
    use vello::peniko::{Fill, Mix};

    // The trim cuts only the line; the alignment clip below keeps the whole
    // outline, so an inside stroke stays inside wherever it is cut.
    let trimmed = trim_outline(outline, stroke);
    let stroked = trimmed.as_ref().unwrap_or(outline);

    if stroke.align == RenderStrokeAlign::Center {
        scene.stroke(
            &vello_stroke(stroke),
            transform,
            vello_color(stroke.color),
            None,
            stroked,
        );
        return;
    }

    let mut doubled = stroke.clone();
    doubled.width = stroke.width * 2.0;

    let clip = match stroke.align {
        RenderStrokeAlign::Inside => outline.clone(),
        // The complement: a rectangle far larger than the shape, with the
        // shape punched out of it by the even-odd rule.
        RenderStrokeAlign::Outside => {
            let bounds = outline.bounding_box().inflate(
                stroke.width * 2.0 + 1.0,
                stroke.width * 2.0 + 1.0,
            );
            let mut path = bounds.to_path(0.1);
            path.extend(outline.iter());
            path
        }
        RenderStrokeAlign::Center => unreachable!("handled above"),
    };
    let rule = match stroke.align {
        RenderStrokeAlign::Outside => Fill::EvenOdd,
        _ => Fill::NonZero,
    };

    scene.push_layer(rule, Mix::Normal, 1.0, transform, &clip);
    scene.stroke(
        &vello_stroke(&doubled),
        transform,
        vello_color(stroke.color),
        None,
        stroked,
    );
    scene.pop_layer();
}

/// Arc-length accuracy for trimming, in user-space units.
#[cfg(feature = "vello-render")]
const TRIM_ACCURACY: f64 = 1e-3;

/// `outline` cut down to the spans `stroke`'s trim draws (issue #158), each
/// subpath trimmed on its own. `None` when the trim is neutral and the
/// outline should be stroked as it is.
#[cfg(feature = "vello-render")]
pub fn trim_outline(
    outline: &vello::kurbo::BezPath,
    stroke: &RenderStroke,
) -> Option<vello::kurbo::BezPath> {
    use vello::kurbo::{BezPath, ParamCurveArclen, PathEl, PathSeg};

    if !stroke.is_trimmed() {
        return None;
    }

    // Split into subpaths; `segments()` then yields each one's closing line.
    let mut subpaths: Vec<BezPath> = Vec::new();
    for element in outline.elements() {
        if matches!(element, PathEl::MoveTo(_)) || subpaths.is_empty() {
            subpaths.push(BezPath::new());
        }
        subpaths.last_mut().expect("pushed above").push(*element);
    }

    let mut out = BezPath::new();
    for subpath in &subpaths {
        let closed = matches!(subpath.elements().last(), Some(PathEl::ClosePath));
        let segments: Vec<PathSeg> = subpath.segments().collect();
        let lengths: Vec<f64> = segments
            .iter()
            .map(|segment| segment.arclen(TRIM_ACCURACY))
            .collect();
        let total: f64 = lengths.iter().sum();
        if total <= 0.0 {
            continue;
        }
        let Some(windows) =
            trim_windows(stroke.trim_start, stroke.trim_end, stroke.trim_offset, closed)
        else {
            out.extend(subpath.iter());
            continue;
        };
        for window in windows {
            let from = window.from * total;
            let to = window.to * total;
            append_arc_range(&mut out, &segments, &lengths, from, to.min(total), true);
            if to > total {
                // Through the seam of a closed subpath: one unbroken piece.
                append_arc_range(&mut out, &segments, &lengths, 0.0, to - total, false);
            }
        }
    }
    Some(out)
}

/// Appends the part of `segments` between arc lengths `from` and `to`,
/// starting a new subpath when `start_new`, else continuing the current one.
#[cfg(feature = "vello-render")]
fn append_arc_range(
    out: &mut vello::kurbo::BezPath,
    segments: &[vello::kurbo::PathSeg],
    lengths: &[f64],
    from: f64,
    to: f64,
    start_new: bool,
) {
    use vello::kurbo::{ParamCurve, ParamCurveArclen, PathSeg};

    let mut needs_move = start_new;
    let mut cursor = 0.0;
    for (segment, &length) in segments.iter().zip(lengths) {
        let (seg_start, seg_end) = (cursor, cursor + length);
        cursor = seg_end;
        let lo = from.max(seg_start);
        let hi = to.min(seg_end);
        if hi <= lo || length <= 0.0 {
            continue;
        }
        let t0 = if lo <= seg_start {
            0.0
        } else {
            segment.inv_arclen(lo - seg_start, TRIM_ACCURACY)
        };
        let t1 = if hi >= seg_end {
            1.0
        } else {
            segment.inv_arclen(hi - seg_start, TRIM_ACCURACY)
        };
        let piece = segment.subsegment(t0..t1);
        if needs_move {
            out.move_to(piece.start());
            needs_move = false;
        }
        match piece {
            PathSeg::Line(line) => out.line_to(line.p1),
            PathSeg::Quad(quad) => out.quad_to(quad.p1, quad.p2),
            PathSeg::Cubic(cubic) => out.curve_to(cubic.p1, cubic.p2, cubic.p3),
        }
    }
}

#[cfg(feature = "vello-render")]
fn vello_mix(mode: RenderBlendMode) -> vello::peniko::Mix {
    use vello::peniko::Mix;
    match mode {
        RenderBlendMode::Normal => Mix::Normal,
        RenderBlendMode::Multiply => Mix::Multiply,
        RenderBlendMode::Screen => Mix::Screen,
        RenderBlendMode::Overlay => Mix::Overlay,
        RenderBlendMode::Darken => Mix::Darken,
        RenderBlendMode::Lighten => Mix::Lighten,
        RenderBlendMode::ColorDodge => Mix::ColorDodge,
        RenderBlendMode::ColorBurn => Mix::ColorBurn,
        RenderBlendMode::HardLight => Mix::HardLight,
        RenderBlendMode::SoftLight => Mix::SoftLight,
        RenderBlendMode::Difference => Mix::Difference,
        RenderBlendMode::Exclusion => Mix::Exclusion,
        RenderBlendMode::Hue => Mix::Hue,
        RenderBlendMode::Saturation => Mix::Saturation,
        RenderBlendMode::Color => Mix::Color,
        RenderBlendMode::Luminosity => Mix::Luminosity,
    }
}

#[cfg(feature = "vello-render")]
fn vello_parent(parent: Affine) -> vello::kurbo::Affine {
    vello::kurbo::Affine::new(parent)
}

#[cfg(feature = "vello-render")]
fn vello_transform(transform: RenderTransform) -> vello::kurbo::Affine {
    use vello::kurbo::{Affine, Vec2};

    Affine::translate(Vec2::new(transform.x, transform.y))
        * Affine::translate(Vec2::new(transform.anchor_x, transform.anchor_y))
        * Affine::rotate(transform.rotation)
        * Affine::scale_non_uniform(transform.scale_x, transform.scale_y)
        * Affine::translate(Vec2::new(-transform.anchor_x, -transform.anchor_y))
}

#[cfg(feature = "vello-render")]
fn vello_path(contours: &[RenderPathContour]) -> vello::kurbo::BezPath {
    use vello::kurbo::BezPath;

    let mut path = BezPath::new();
    for (contour, decomposed) in contours.iter().zip(path_segments(contours)) {
        let Some(first) = contour.points.first() else {
            continue;
        };
        path.move_to((first.anchor_x, first.anchor_y));

        for (index, segment) in decomposed.segments.iter().enumerate() {
            // The closing segment is drawn by `close_path`, which draws a
            // straight line — so only emit it here when it actually curves.
            let is_closing = decomposed.closed && index == decomposed.segments.len() - 1;
            let straight = segment.control1 == segment.start && segment.control2 == segment.end;
            if is_closing && straight {
                continue;
            }
            if straight {
                path.line_to(segment.end);
            } else {
                path.curve_to(segment.control1, segment.control2, segment.end);
            }
        }

        if contour.closed {
            path.close_path();
        }
    }
    path
}

/// Resolves a [`RenderPaint`] against the bounds of the shape it fills.
///
/// [`RenderPaint`] stores gradient geometry normalised to the node's box, the
/// way the editor authors it, so this is where 0..1 becomes user space. The
/// radius scales by the box's longer side and the focal point is honoured,
/// both to match `frame_renderer.dart:748-765` — the runtime and the editor
/// have to agree on the gradient's shape, not merely both draw one.
/// The vello stroke style for a [`RenderStroke`].
///
/// `RenderStroke` was `{color, width}`, so cap, join, mitre limit and dashes
/// were read out of the file and thrown away: every stroke drew butt-capped,
/// mitre-joined and solid however it was authored.
#[cfg(feature = "vello-render")]
fn vello_stroke(stroke: &RenderStroke) -> vello::kurbo::Stroke {
    use vello::kurbo::{Cap, Join, Stroke};

    let style = Stroke::new(stroke.width)
        .with_caps(match stroke.cap {
            RenderCap::Butt => Cap::Butt,
            RenderCap::Round => Cap::Round,
            RenderCap::Square => Cap::Square,
        })
        .with_join(match stroke.join {
            RenderJoin::Miter => Join::Miter,
            RenderJoin::Round => Join::Round,
            RenderJoin::Bevel => Join::Bevel,
        })
        .with_miter_limit(stroke.miter_limit);

    if stroke.dash_array.is_empty() {
        style
    } else {
        style.with_dashes(stroke.dash_offset, stroke.dash_array.iter().copied())
    }
}

/// A [`RenderTransform`] as a plain affine.
///
/// The same composition `vello_transform` builds, without the vello feature —
/// skinning has to work on every backend, and on none at all in tests.
fn render_affine(transform: RenderTransform) -> Affine {
    use crate::hierarchy::multiply;

    let translate = |x: f64, y: f64| -> Affine { [1.0, 0.0, 0.0, 1.0, x, y] };
    let (sin, cos) = transform.rotation.sin_cos();

    let mut affine = translate(transform.x, transform.y);
    affine = multiply(affine, translate(transform.anchor_x, transform.anchor_y));
    affine = multiply(affine, [cos, sin, -sin, cos, 0.0, 0.0]);
    affine = multiply(
        affine,
        [transform.scale_x, 0.0, 0.0, transform.scale_y, 0.0, 0.0],
    );
    multiply(
        affine,
        translate(-transform.anchor_x, -transform.anchor_y),
    )
}

/// The node's world matrix and its inverse, for skinning.
///
/// Bone matrices are world-space, so a vertex has to travel out of node space
/// and back. A non-invertible world matrix (a zero scale) returns identity
/// both ways, which deforms in node space rather than dividing by zero.
fn skinning_space(
    parent: Affine,
    transform: RenderTransform,
) -> (crate::bones::Matrix4, crate::bones::Matrix4) {
    let local_to_world = crate::hierarchy::multiply(parent, render_affine(transform));
    match crate::hierarchy::invert(local_to_world) {
        Some(inverse) => (
            matrix4_from_affine(local_to_world),
            matrix4_from_affine(inverse),
        ),
        None => (IDENTITY_MATRIX4, IDENTITY_MATRIX4),
    }
}

/// The mask's outline as a path, in its own local space.
#[cfg(feature = "vello-render")]
fn shape_path(shape: &RenderShape) -> vello::kurbo::BezPath {
    use vello::kurbo::{Ellipse, Rect, Shape as _};

    match shape {
        RenderShape::Rectangle {
            width,
            height,
            corner_radius,
        } => {
            let rect = Rect::new(0.0, 0.0, *width, *height);
            let radius = corner_radius.min(width.min(*height) / 2.0).max(0.0);
            if radius > 0.0 {
                rect.to_rounded_rect(radius).to_path(0.1)
            } else {
                rect.to_path(0.1)
            }
        }
        RenderShape::Ellipse { width, height } => {
            Ellipse::new((width / 2.0, height / 2.0), (width / 2.0, height / 2.0), 0.0).to_path(0.1)
        }
        RenderShape::Path { contours, .. } => vello_path(contours),
        // A mask made of an image clips to its frame; its alpha is not
        // consulted, which is the same geometric-clip limitation every other
        // mask shape has here.
        RenderShape::Image { width, height, .. } => {
            Rect::new(0.0, 0.0, *width, *height).to_path(0.1)
        }
        // Unreachable: `attach_clip_masks` never builds a clip from text,
        // because there is no assembled outline to clip with. An empty path
        // here would clip the content away entirely, which is the opposite of
        // the intended fallback.
        RenderShape::Text { .. } => vello::kurbo::BezPath::new(),
    }
}

/// Wraps decoded pixels in a vello image brush.
#[cfg(feature = "vello-render")]
fn vello_image(rgba: &[u8], width: u32, height: u32) -> Option<vello::peniko::ImageBrush> {
    use vello::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};

    // A truncated buffer would read past the pixels it has, so a short decode
    // draws nothing rather than garbage.
    if width == 0 || height == 0 || rgba.len() < (width as usize * height as usize * 4) {
        return None;
    }
    Some(ImageBrush::new(ImageData {
        data: Blob::from(rgba.to_vec()),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width,
        height,
    }))
}

#[cfg(feature = "vello-render")]
fn vello_brush(paint: &RenderPaint, bounds: vello::kurbo::Rect) -> vello::peniko::Brush {
    use vello::peniko::{Brush, ColorStop, Extend, Gradient};

    fn extend(spread: RenderSpread) -> Extend {
        match spread {
            RenderSpread::Pad => Extend::Pad,
            RenderSpread::Reflect => Extend::Reflect,
            RenderSpread::Repeat => Extend::Repeat,
        }
    }

    fn color_stops(stops: &[RenderGradientStop]) -> Vec<ColorStop> {
        stops
            .iter()
            .map(|stop| ColorStop {
                offset: stop.offset as f32,
                color: vello_color(stop.color).into(),
            })
            .collect()
    }

    let point = |(x, y): (f64, f64)| {
        vello::kurbo::Point::new(
            bounds.x0 + x * bounds.width(),
            bounds.y0 + y * bounds.height(),
        )
    };

    match paint {
        RenderPaint::Solid(color) => Brush::Solid(vello_color(*color)),
        RenderPaint::LinearGradient {
            stops,
            from,
            to,
            spread,
        } => Brush::Gradient(
            Gradient::new_linear(point(*from), point(*to))
                .with_extend(extend(*spread))
                .with_stops(&color_stops(stops)[..]),
        ),
        RenderPaint::RadialGradient {
            stops,
            center,
            focal,
            radius,
            spread,
        } => {
            let r = (*radius * bounds.width().max(bounds.height())) as f32;
            Brush::Gradient(
                // Focus and centre as a two-point radial: a zero-radius circle
                // at the focus growing to the full circle at the centre, which
                // is how dart:ui's `focal` parameter behaves.
                Gradient::new_two_point_radial(point(*focal), 0.0, point(*center), r)
                    .with_extend(extend(*spread))
                    .with_stops(&color_stops(stops)[..]),
            )
        }
    }
}

#[cfg(feature = "vello-render")]
fn vello_color(color: RenderColor) -> vello::peniko::Color {
    vello::peniko::Color::from_rgba8(color.r, color.g, color.b, color.a)
}

#[cfg(all(test, feature = "vello-render"))]
mod path_tests {
    use super::*;

    /// Issue #111 device QA: paths rendered as spikes fanning out from the
    /// top-left while rectangles came out fine.
    ///
    /// Handles are stored relative to their anchor. Treating them as absolute
    /// control points put every control near the origin — which is exactly
    /// what a spike from the origin is.
    fn point(
        anchor: (f64, f64),
        in_handle: (f64, f64),
        out_handle: (f64, f64),
    ) -> RenderPathPoint {
        RenderPathPoint {
            anchor_x: anchor.0,
            anchor_y: anchor.1,
            in_handle_x: in_handle.0,
            in_handle_y: in_handle.1,
            out_handle_x: out_handle.0,
            out_handle_y: out_handle.1,
        }
    }

    #[test]
    fn a_relative_handle_becomes_an_absolute_control_point() {
        let contour = RenderPathContour {
            points: vec![
                point((100.0, 100.0), (0.0, 0.0), (50.0, 0.0)),
                point((300.0, 100.0), (-50.0, 0.0), (0.0, 0.0)),
            ],
            closed: false,
        };

        let path = vello_path(&[contour]);
        let elements: Vec<_> = path.elements().to_vec();

        assert_eq!(elements.len(), 2, "a move and one curve");
        match elements[1] {
            vello::kurbo::PathEl::CurveTo(c1, c2, end) => {
                // anchor + handle, not the handle on its own.
                assert_eq!((c1.x, c1.y), (150.0, 100.0));
                assert_eq!((c2.x, c2.y), (250.0, 100.0));
                assert_eq!((end.x, end.y), (300.0, 100.0));
            }
            ref other => panic!("expected a curve, got {other:?}"),
        }
    }

    #[test]
    fn a_zero_handle_stays_a_straight_line() {
        let contour = RenderPathContour {
            points: vec![
                point((0.0, 0.0), (0.0, 0.0), (0.0, 0.0)),
                point((100.0, 0.0), (0.0, 0.0), (0.0, 0.0)),
            ],
            closed: false,
        };

        let path = vello_path(&[contour]);
        let elements: Vec<_> = path.elements().to_vec();

        // The old test asked whether an absolute handle sat on its anchor,
        // which is almost never true — so straight segments were curved
        // through the origin.
        assert!(
            matches!(elements[1], vello::kurbo::PathEl::LineTo(_)),
            "a handle of (0,0) means the segment is straight, got {:?}",
            elements[1]
        );
    }
}
