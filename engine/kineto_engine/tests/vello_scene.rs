use kineto_engine::format;
use kineto_engine::format::generated::kineto::{
    finish_document_buffer, AnimatedPropertyKind, Animation, AnimationArgs, Camera, CameraArgs,
    Color, Component, CompoundPath, CompoundPathArgs, Document, DocumentArgs, DropShadow,
    DropShadowArgs, EffectEntry, EffectEntryArgs, EffectKind, FillKind, FillV2, FillV2Args,
    FontFace, FontFaceArgs, Geometry, GradientStop, GradientStopArgs, Group, GroupArgs, Image,
    ImageArgs, KFKeyframe, KFKeyframeArgs, KFTrack, KFTrackArgs, KeyframeInterpolationKind,
    LinearGradientFill, LinearGradientFillArgs, Node, NodeArgs, Path, PathArgs, PathContour,
    PathContourArgs, PathFillRule, PathNodeFBV2, PathNodeFBV2Args, RadialGradientFill,
    RadialGradientFillArgs, Rectangle, RectangleArgs, Shape, ShapeArgs, SolidFill, SolidFillArgs,
    SpreadMethod, Stroke, StrokeAlign, StrokeArgs, StrokeCap, StrokeJoin, Text, TextAlign, TextArgs,
    Transform, TransformArgs,
};
use kineto_engine::animation::timeline_eval::PropertyValue;
use kineto_engine::player::KinetoPlayer;
use kineto_engine::render::{
    build_render_scene, fit_transform, RenderCap, RenderColor, RenderJoin, RenderPaint,
    RenderShape, RenderSpread, RenderStrokeAlign,
};

fn assert_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {actual} to be within {epsilon} of {expected}"
    );
}

#[test]
fn builds_render_scene_from_evaluated_shape_state() {
    let bytes = build_render_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    assert_eq!(scene.commands.len(), 3);
    assert_eq!(scene.commands[0].node_id, "background");
    assert_eq!(scene.commands[1].node_id, "button");

    let button = &scene.commands[1];
    assert_close(button.transform.x, 30.0, 1e-9);
    assert_close(button.transform.y, 20.0, 1e-9);
    assert_close(button.opacity, 0.5, 1e-9);
    assert_eq!(
        button.fills,
        vec![RenderPaint::Solid(RenderColor {
            r: 0,
            g: 255,
            b: 0,
            a: 128
        })]
    );
    assert_eq!(
        button.stroke.as_ref().expect("stroke should render").color,
        RenderColor {
            r: 255,
            g: 0,
            b: 0,
            a: 128
        }
    );
    assert_close(
        button.stroke.as_ref().expect("stroke should render").width,
        4.0,
        1e-9,
    );
    // Issue #158: a trim track reaches the stroke the way a width track does.
    assert_close(
        button.stroke.as_ref().expect("stroke should render").trim_end,
        0.4,
        1e-6,
    );

    match button.shape {
        RenderShape::Rectangle { width, height, .. } => {
            assert_close(width, 60.0, 1e-9);
            assert_close(height, 20.0, 1e-9);
        }
        ref other => panic!("expected rectangle, got {other:?}"),
    }
}

#[test]
fn builds_render_scene_from_checked_in_runtime_sample() {
    let bytes = include_bytes!("../../../packages/kineto_runtime/example/assets/sample.kvk");
    let document = format::load(bytes).expect("sample document should parse");
    let mut player = KinetoPlayer::load(bytes.to_vec()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    assert_eq!(scene.commands.len(), 1);
    assert_eq!(scene.commands[0].node_id, "n1");
}

#[test]
fn includes_path_contours_and_skips_hidden_nodes() {
    let bytes = build_render_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);
    let path = scene
        .commands
        .iter()
        .find(|command| command.node_id == "triangle")
        .expect("path command should exist");

    match &path.shape {
        RenderShape::Path { contours, .. } => {
            assert_eq!(contours.len(), 1);
            assert!(contours[0].closed);
            assert_eq!(contours[0].points.len(), 3);
            assert_close(contours[0].points[2].anchor_x, 40.0, 1e-9);
            assert_close(contours[0].points[2].anchor_y, 40.0, 1e-9);
        }
        other => panic!("expected path, got {other:?}"),
    }
    assert!(scene
        .commands
        .iter()
        .all(|command| command.node_id != "hidden"));
}

#[cfg(feature = "vello-render")]
#[test]
fn builds_vello_scene_without_gpu_backend() {
    let bytes = build_render_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");
    let scene = build_render_scene(document, &frame);

    let _vello_scene = kineto_engine::render::build_vello_scene(&scene);
}

#[cfg(feature = "metal-render")]
#[test]
fn rejects_zero_sized_metal_surfaces() {
    assert_eq!(kineto_engine::render::SurfaceSize::new(0, 100), None);
    assert_eq!(kineto_engine::render::SurfaceSize::new(100, 0), None);
    assert_eq!(
        kineto_engine::render::SurfaceSize::new(100, 50),
        Some(kineto_engine::render::SurfaceSize {
            width: 100,
            height: 50
        })
    );
}

fn build_render_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);
    let nodes = build_nodes(&mut fbb);
    let animations = build_animations(&mut fbb);
    let name = fbb.create_string("Render Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 120.0,
            nodes: Some(nodes),
            animations: Some(animations),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

fn build_nodes<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<Node<'a>>>> {
    let background = rect_node(
        fbb,
        "background",
        "Background",
        0.0,
        0.0,
        200.0,
        120.0,
        Some(RenderColor {
            r: 20,
            g: 30,
            b: 40,
            a: 255,
        }),
        true,
    );
    let button = rect_node(
        fbb,
        "button",
        "Button",
        10.0,
        20.0,
        40.0,
        20.0,
        Some(RenderColor {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }),
        true,
    );
    let hidden = rect_node(fbb, "hidden", "Hidden", 5.0, 5.0, 15.0, 15.0, None, false);
    let triangle = path_node(fbb);
    fbb.create_vector(&[background, button, hidden, triangle])
}

fn rect_node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    fill: Option<RenderColor>,
    visible: bool,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let rect = Rectangle::create(
        fbb,
        &RectangleArgs {
            width,
            height,
            ..Default::default()
        },
    );
    let fill_v2 = fill.map(|color| solid_fill(fbb, color, 1.0));
    let stroke_color = Color::new(1, 2, 3, 255);
    let stroke = Stroke::create(
        fbb,
        &StrokeArgs {
            color: Some(&stroke_color),
            width: 2.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            fill_v2,
            stroke: Some(stroke),
            ..Default::default()
        },
    );
    node(
        fbb,
        id,
        name,
        x,
        y,
        Component::Shape,
        shape.as_union_value(),
        visible,
    )
}

fn path_node<'a>(fbb: &mut flatbuffers::FlatBufferBuilder<'a>) -> flatbuffers::WIPOffset<Node<'a>> {
    let p0 = path_point(fbb, 0.0, 0.0);
    let p1 = path_point(fbb, 40.0, 0.0);
    let p2 = path_point(fbb, 40.0, 40.0);
    let points = fbb.create_vector(&[p0, p1, p2]);
    let fill_v2 = solid_fill(
        fbb,
        RenderColor {
            r: 120,
            g: 40,
            b: 200,
            a: 255,
        },
        1.0,
    );
    let path = Path::create(
        fbb,
        &PathArgs {
            nodes_v2: Some(points),
            closed: true,
            fill_v2: Some(fill_v2),
            ..Default::default()
        },
    );
    node(
        fbb,
        "triangle",
        "Triangle",
        70.0,
        10.0,
        Component::Path,
        path.as_union_value(),
        true,
    )
}

fn path_point<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    x: f32,
    y: f32,
) -> flatbuffers::WIPOffset<PathNodeFBV2<'a>> {
    PathNodeFBV2::create(
        fbb,
        &PathNodeFBV2Args {
            anchor_x: x,
            anchor_y: y,
            in_handle_x: x,
            in_handle_y: y,
            out_handle_x: x,
            out_handle_y: y,
            ..Default::default()
        },
    )
}

fn solid_fill<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    color: RenderColor,
    opacity: f32,
) -> flatbuffers::WIPOffset<FillV2<'a>> {
    let color = Color::new(color.r, color.g, color.b, color.a);
    let solid = SolidFill::create(
        fbb,
        &SolidFillArgs {
            color: Some(&color),
            opacity,
        },
    );
    FillV2::create(
        fbb,
        &FillV2Args {
            kind_type: FillKind::SolidFill,
            kind: Some(solid.as_union_value()),
        },
    )
}

fn node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    x: f32,
    y: f32,
    component_type: Component,
    component: flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>,
    visible: bool,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let transform = Transform::create(
        fbb,
        &TransformArgs {
            x,
            y,
            ..Default::default()
        },
    );
    let id = fbb.create_string(id);
    let name = fbb.create_string(name);
    Node::create(
        fbb,
        &NodeArgs {
            id: Some(id),
            name: Some(name),
            component_type,
            component: Some(component),
            transform: Some(transform),
            visible,
            opacity: 1.0,
            ..Default::default()
        },
    )
}

fn build_animations<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<Animation<'a>>>> {
    let tracks = [
        scalar_track(fbb, "track-x", AnimatedPropertyKind::PositionX, 30.0),
        scalar_track(fbb, "track-opacity", AnimatedPropertyKind::Opacity, 0.5),
        scalar_track(fbb, "track-width", AnimatedPropertyKind::Width, 60.0),
        scalar_track(
            fbb,
            "track-stroke-width",
            AnimatedPropertyKind::StrokeWidth,
            4.0,
        ),
        scalar_track(
            fbb,
            "track-stroke-trim-end",
            AnimatedPropertyKind::StrokeTrimEnd,
            0.4,
        ),
        color_track(
            fbb,
            "track-fill",
            AnimatedPropertyKind::FillColor,
            0xFF00FF00,
        ),
        color_track(
            fbb,
            "track-stroke",
            AnimatedPropertyKind::StrokeColor,
            0xFFFF0000,
        ),
    ];
    let tracks = fbb.create_vector(&tracks);
    let id = fbb.create_string("idle");
    let name = fbb.create_string("Idle");
    let animation = Animation::create(
        fbb,
        &AnimationArgs {
            id: Some(id),
            name: Some(name),
            duration: 1.0,
            tracks: Some(tracks),
            ..Default::default()
        },
    );
    fbb.create_vector(&[animation])
}

fn scalar_track<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    property: AnimatedPropertyKind,
    value: f32,
) -> flatbuffers::WIPOffset<KFTrack<'a>> {
    track(
        fbb,
        id,
        property,
        KFKeyframeArgs {
            scalar_value: value,
            ..Default::default()
        },
    )
}

fn color_track<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    property: AnimatedPropertyKind,
    value: u32,
) -> flatbuffers::WIPOffset<KFTrack<'a>> {
    track(
        fbb,
        id,
        property,
        KFKeyframeArgs {
            color_value: value,
            is_color: true,
            ..Default::default()
        },
    )
}

fn track<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    property: AnimatedPropertyKind,
    mut keyframe_args: KFKeyframeArgs<'a>,
) -> flatbuffers::WIPOffset<KFTrack<'a>> {
    let keyframe_id = fbb.create_string(id);
    keyframe_args.id = Some(keyframe_id);
    keyframe_args.time = 0.0;
    keyframe_args.interpolation = KeyframeInterpolationKind::Linear;
    let keyframe = KFKeyframe::create(fbb, &keyframe_args);
    let keyframes = fbb.create_vector(&[keyframe]);
    let track_id = fbb.create_string(id);
    let target = fbb.create_string("button");
    KFTrack::create(
        fbb,
        &KFTrackArgs {
            id: Some(track_id),
            target_node_id: Some(target),
            property,
            keyframes: Some(keyframes),
        },
    )
}

/// A rounded rectangle authored in the editor was drawn with square corners
/// because the render shape dropped `corner_radius` on the floor.
#[test]
fn carries_the_authored_corner_radius_into_the_render_shape() {
    let bytes = build_corner_radius_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    match scene.commands[0].shape {
        RenderShape::Rectangle { corner_radius, .. } => {
            assert_close(corner_radius, 12.0, 1e-9)
        }
        ref other => panic!("expected a rectangle, got {other:?}"),
    }
}

/// An even-odd compound path (a donut) was filled non-zero, so its hole
/// disappeared.
#[test]
fn carries_the_even_odd_fill_rule_into_the_render_shape() {
    let bytes = build_even_odd_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    match scene.commands[0].shape {
        RenderShape::Path { even_odd, .. } => assert!(even_odd, "donut should fill even-odd"),
        ref other => panic!("expected a path, got {other:?}"),
    }
}

/// A hidden group left its children on screen, and a faded group did not fade
/// them, because visibility and opacity were read per node without walking up.
#[test]
fn inherits_visibility_and_opacity_from_enclosing_groups() {
    let bytes = build_group_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    let ids: Vec<&str> = scene.commands.iter().map(|c| c.node_id.as_str()).collect();
    assert!(
        !ids.contains(&"in-hidden-group"),
        "a child of a hidden group must not draw, got {ids:?}"
    );

    let faded = scene
        .commands
        .iter()
        .find(|c| c.node_id == "in-faded-group")
        .expect("the child of the faded group should still draw");
    // 0.5 on the group times 0.5 on the child.
    assert_close(faded.opacity, 0.25, 1e-9);
}

fn wrap_document<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    name: &'static str,
    nodes: flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<Node<'a>>>>,
) -> Vec<u8> {
    let name = fbb.create_string(name);
    let document = Document::create(
        fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 120.0,
            nodes: Some(nodes),
            ..Default::default()
        },
    );
    finish_document_buffer(fbb, document);
    fbb.finished_data().to_vec()
}

fn build_corner_radius_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 80.0,
            height: 40.0,
            corner_radius: 12.0,
        },
    );
    let fill = solid_fill(
        &mut fbb,
        RenderColor {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        },
        1.0,
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            fill_v2: Some(fill),
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "rounded",
        "Rounded",
        0.0,
        0.0,
        Component::Shape,
        shape.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);
    wrap_document(&mut fbb, "Corner Radius Fixture", nodes)
}

fn build_even_odd_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let outer = contour(&mut fbb, 0.0, 0.0, 40.0);
    let inner = contour(&mut fbb, 10.0, 10.0, 20.0);
    let contours = fbb.create_vector(&[outer, inner]);
    let fill = solid_fill(
        &mut fbb,
        RenderColor {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        },
        1.0,
    );
    let path = CompoundPath::create(
        &mut fbb,
        &CompoundPathArgs {
            contours: Some(contours),
            fill_rule: PathFillRule::EvenOdd,
            fill_v2: Some(fill),
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "donut",
        "Donut",
        0.0,
        0.0,
        Component::CompoundPath,
        path.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);
    wrap_document(&mut fbb, "Even Odd Fixture", nodes)
}

/// A square contour, so the fixture reads as two rings rather than a shape the
/// fill rule cannot distinguish.
fn contour<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    x: f32,
    y: f32,
    size: f32,
) -> flatbuffers::WIPOffset<PathContour<'a>> {
    let points = [
        path_point(fbb, x, y),
        path_point(fbb, x + size, y),
        path_point(fbb, x + size, y + size),
        path_point(fbb, x, y + size),
    ];
    let nodes_v2 = fbb.create_vector(&points);
    PathContour::create(
        fbb,
        &PathContourArgs {
            nodes_v2: Some(nodes_v2),
            closed: true,
            ..Default::default()
        },
    )
}

fn build_group_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);

    let hidden_child = rect_node(
        &mut fbb,
        "in-hidden-group",
        "In Hidden Group",
        0.0,
        0.0,
        10.0,
        10.0,
        None,
        true,
    );
    let faded_child = faded_rect_node(&mut fbb, "in-faded-group", "In Faded Group", 0.5);

    let hidden_group = group_node(&mut fbb, "hidden-group", "Hidden Group", "in-hidden-group", 1.0, false);
    let faded_group = group_node(&mut fbb, "faded-group", "Faded Group", "in-faded-group", 0.5, true);

    let nodes = fbb.create_vector(&[hidden_group, hidden_child, faded_group, faded_child]);
    wrap_document(&mut fbb, "Group Fixture", nodes)
}

fn group_node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    child_id: &'static str,
    opacity: f32,
    visible: bool,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let child = fbb.create_string(child_id);
    let child_ids = fbb.create_vector(&[child]);
    let group = Group::create(
        fbb,
        &GroupArgs {
            child_ids: Some(child_ids),
            mask_child_id: None,
            solo_child_id: None,
        },
    );
    let transform = Transform::create(fbb, &TransformArgs::default());
    let id = fbb.create_string(id);
    let name = fbb.create_string(name);
    Node::create(
        fbb,
        &NodeArgs {
            id: Some(id),
            name: Some(name),
            component_type: Component::Group,
            component: Some(group.as_union_value()),
            transform: Some(transform),
            visible,
            opacity,
            ..Default::default()
        },
    )
}

fn faded_rect_node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    name: &'static str,
    opacity: f32,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let rect = Rectangle::create(
        fbb,
        &RectangleArgs {
            width: 10.0,
            height: 10.0,
            ..Default::default()
        },
    );
    let fill = solid_fill(
        fbb,
        RenderColor {
            r: 9,
            g: 9,
            b: 9,
            a: 255,
        },
        1.0,
    );
    let shape = Shape::create(
        fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            fill_v2: Some(fill),
            ..Default::default()
        },
    );
    let transform = Transform::create(fbb, &TransformArgs::default());
    let id = fbb.create_string(id);
    let name = fbb.create_string(name);
    Node::create(
        fbb,
        &NodeArgs {
            id: Some(id),
            name: Some(name),
            component_type: Component::Shape,
            component: Some(shape.as_union_value()),
            transform: Some(transform),
            visible: true,
            opacity,
            ..Default::default()
        },
    )
}

/// A gradient node used to draw with no fill at all: the fill reader unwrapped
/// `kind_as_solid_fill()?` and gave up on anything else.
#[test]
fn reads_a_linear_gradient_fill_instead_of_drawing_nothing() {
    let bytes = build_gradient_doc(false);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    match scene.commands[0].fills.first().expect("gradient should fill") {
        RenderPaint::LinearGradient {
            stops,
            from,
            to,
            spread,
        } => {
            assert_eq!(stops.len(), 2);
            assert_close(stops[0].offset, 0.0, 1e-6);
            assert_eq!(stops[0].color, RenderColor { r: 255, g: 0, b: 0, a: 255 });
            assert_close(stops[1].offset, 1.0, 1e-6);
            assert_eq!(stops[1].color, RenderColor { r: 0, g: 0, b: 255, a: 255 });
            // Normalised to the node's box, the way the editor authors it.
            assert_close(from.0, 0.0, 1e-6);
            assert_close(from.1, 0.5, 1e-6);
            assert_close(to.0, 1.0, 1e-6);
            assert_close(to.1, 0.5, 1e-6);
            assert_eq!(*spread, RenderSpread::Reflect);
        }
        other => panic!("expected a linear gradient, got {other:?}"),
    }
}

#[test]
fn reads_a_radial_gradient_fill() {
    let bytes = build_gradient_doc(true);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    match scene.commands[0].fills.first().expect("gradient should fill") {
        RenderPaint::RadialGradient {
            center,
            focal,
            radius,
            ..
        } => {
            assert_close(center.0, 0.5, 1e-6);
            assert_close(center.1, 0.5, 1e-6);
            assert_close(focal.0, 0.25, 1e-6);
            assert_close(focal.1, 0.75, 1e-6);
            assert_close(*radius, 0.4, 1e-6);
        }
        other => panic!("expected a radial gradient, got {other:?}"),
    }
}

/// A `fillColor` track names one colour, and a gradient has many. Flattening
/// it here would be the same silent-substitution mistake the export aliases
/// were; recolouring stops is the editor's separate `gradientStopColor`
/// property, which the pre-flight reports as unsupported.
#[test]
fn a_fill_colour_track_does_not_flatten_a_gradient() {
    let bytes = build_gradient_doc(false);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let mut frame = player.play().expect("frame should evaluate");
    frame
        .node_overrides
        .entry("gradient".to_string())
        .or_default()
        .values
        .insert("fillColor", PropertyValue::Color(0xFF00FF00));

    let scene = build_render_scene(document, &frame);

    assert!(matches!(
        scene.commands[0].fills.as_slice(),
        [RenderPaint::LinearGradient { .. }]
    ));
}

fn build_gradient_doc(radial: bool) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let red = Color::new(255, 0, 0, 255);
    let blue = Color::new(0, 0, 255, 255);
    let stop0 = GradientStop::create(
        &mut fbb,
        &GradientStopArgs {
            offset: 0.0,
            color: Some(&red),
            opacity: 1.0,
        },
    );
    let stop1 = GradientStop::create(
        &mut fbb,
        &GradientStopArgs {
            offset: 1.0,
            color: Some(&blue),
            opacity: 1.0,
        },
    );
    let stops = fbb.create_vector(&[stop0, stop1]);

    let (kind_type, kind) = if radial {
        let radial = RadialGradientFill::create(
            &mut fbb,
            &RadialGradientFillArgs {
                stops: Some(stops),
                cx: 0.5,
                cy: 0.5,
                fx: 0.25,
                fy: 0.75,
                r: 0.4,
                ..Default::default()
            },
        );
        (FillKind::RadialGradientFill, radial.as_union_value())
    } else {
        let linear = LinearGradientFill::create(
            &mut fbb,
            &LinearGradientFillArgs {
                stops: Some(stops),
                p0x: 0.0,
                p0y: 0.5,
                p1x: 1.0,
                p1y: 0.5,
                spread: SpreadMethod::Reflect,
                ..Default::default()
            },
        );
        (FillKind::LinearGradientFill, linear.as_union_value())
    };
    let fill = FillV2::create(
        &mut fbb,
        &FillV2Args {
            kind_type,
            kind: Some(kind),
        },
    );

    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 100.0,
            height: 50.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            fill_v2: Some(fill),
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "gradient",
        "Gradient",
        0.0,
        0.0,
        Component::Shape,
        shape.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);
    wrap_document(&mut fbb, "Gradient Fixture", nodes)
}

/// The exporter has always written the canvas size and the engine never read
/// it, so a document drew 1:1 in the corner of a larger drawable.
#[test]
fn fits_the_document_into_the_drawable_without_distorting_it() {
    let bytes = build_render_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);
    assert_eq!(scene.canvas, (200.0, 120.0));

    // A 400x400 drawable: the tighter axis is the width (2.0 vs 3.33), so the
    // document scales by 2 and the leftover height is split evenly.
    let fit = fit_transform(scene.canvas, (400.0, 400.0));
    assert_close(fit[0], 2.0, 1e-9);
    assert_close(fit[3], 2.0, 1e-9);
    assert_close(fit[1], 0.0, 1e-9);
    assert_close(fit[2], 0.0, 1e-9);
    assert_close(fit[4], 0.0, 1e-9);
    assert_close(fit[5], 80.0, 1e-9);
}

#[test]
fn a_degenerate_surface_draws_one_to_one_rather_than_collapsing() {
    // A surface that has not been laid out yet must not scale the document to
    // nothing — the frame would be blank with no error to point at.
    assert_eq!(fit_transform((200.0, 120.0), (0.0, 0.0)), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(fit_transform((0.0, 0.0), (400.0, 400.0)), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
}

/// GroupNode.maskChildId had no schema field until issue #111, so masked
/// artwork exported and drew unmasked.
#[test]
fn clips_a_groups_children_to_its_mask() {
    let bytes = build_mask_doc("mask", &["mask", "art"]);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    let art = scene
        .commands
        .iter()
        .find(|c| c.node_id == "art")
        .expect("the artwork should draw");
    let clip = art.clip.as_ref().expect("the artwork should be clipped");
    assert_eq!(clip.node_id, "mask");

    // A creative-app clipping mask keeps its base visible, and the base is not
    // clipped by itself.
    let mask = scene
        .commands
        .iter()
        .find(|c| c.node_id == "mask")
        .expect("the mask should still draw as artwork");
    assert!(mask.clip.is_none());
}

/// Mirrors `effectiveMaskChildId` (`document_helpers.dart:245`): with nothing
/// above it, clipping would composite the group away to nothing, so the lone
/// mask paints as ordinary artwork instead.
#[test]
fn a_group_whose_only_child_is_its_mask_clips_nothing() {
    let bytes = build_mask_doc("mask", &["mask"]);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    assert!(scene.commands.iter().all(|c| c.clip.is_none()));
    assert!(scene.commands.iter().any(|c| c.node_id == "mask"));
}

/// The mask id must name a *direct child*. "outsider" is a real, drawable
/// node elsewhere in the document, so without that rule the group's children
/// would be clipped to an unrelated shape rather than left alone.
#[test]
fn a_mask_id_that_names_a_node_outside_the_group_clips_nothing() {
    let bytes = build_mask_doc("outsider", &["mask", "art"]);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    assert!(scene.commands.iter().all(|c| c.clip.is_none()));
}

/// The exporter lists a group's members before the group, so drawing in
/// `doc.nodes` order put a root shape listed after a root group (a background
/// behind a character) on top of the whole group. The editor paints roots in
/// `doc.nodes` order and each group in its `children` order.
#[test]
fn paints_in_tree_order_not_flat_document_order() {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);
    let art = rect_node(&mut fbb, "art", "Art", 0.0, 0.0, 50.0, 50.0, None, true);
    let group = stack_group(&mut fbb, "group", &["art"], None);
    let background = rect_node(&mut fbb, "background", "Background", 0.0, 0.0, 100.0, 100.0, None, true);
    // Exporter order: member, then the root background, then its group.
    let nodes = fbb.create_vector(&[art, background, group]);
    let bytes = wrap_document(&mut fbb, "Order Fixture", nodes);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    let order: Vec<&str> = scene.commands.iter().map(|c| c.node_id.as_str()).collect();
    assert_eq!(order, vec!["background", "art"]);
}

/// Artwork usually sits in its own group inside the clip group (an iris in a
/// group beside the eye white that clips it). Only the direct children were
/// clipped, and a group draws nothing, so the iris drew unmasked.
#[test]
fn clips_artwork_nested_inside_a_clip_groups_child_group() {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);
    let mask = rect_node(&mut fbb, "mask", "Mask", 0.0, 0.0, 50.0, 50.0, None, true);
    let art = rect_node(&mut fbb, "art", "Art", 0.0, 0.0, 100.0, 100.0, None, true);
    let inner = stack_group(&mut fbb, "inner", &["art"], None);
    let clip = stack_group(&mut fbb, "clip", &["inner", "mask"], Some("mask"));
    let nodes = fbb.create_vector(&[art, inner, mask, clip]);
    let bytes = wrap_document(&mut fbb, "Nested Mask Fixture", nodes);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    let art = scene.commands.iter().find(|c| c.node_id == "art").expect("art should draw");
    assert_eq!(art.clip.as_ref().map(|c| c.node_id.as_str()), Some("mask"));
    let mask = scene.commands.iter().find(|c| c.node_id == "mask").expect("mask should draw");
    assert!(mask.clip.is_none());
}

fn stack_group<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &'static str,
    child_ids: &[&str],
    mask_id: Option<&str>,
) -> flatbuffers::WIPOffset<Node<'a>> {
    let children: Vec<_> = child_ids.iter().map(|id| fbb.create_string(id)).collect();
    let children = fbb.create_vector(&children);
    let mask_ref = mask_id.map(|mask| fbb.create_string(mask));
    let group = Group::create(
        fbb,
        &GroupArgs {
            child_ids: Some(children),
            mask_child_id: mask_ref,
            solo_child_id: None,
        },
    );
    let transform = Transform::create(fbb, &TransformArgs::default());
    let id = fbb.create_string(id);
    let name = fbb.create_string("Group");
    Node::create(
        fbb,
        &NodeArgs {
            id: Some(id),
            name: Some(name),
            component_type: Component::Group,
            component: Some(group.as_union_value()),
            transform: Some(transform),
            visible: true,
            opacity: 1.0,
            ..Default::default()
        },
    )
}

fn build_mask_doc(mask_id: &str, child_ids: &[&str]) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);

    let mask = rect_node(&mut fbb, "mask", "Mask", 0.0, 0.0, 50.0, 50.0, None, true);
    let art = rect_node(&mut fbb, "art", "Art", 0.0, 0.0, 100.0, 100.0, None, true);

    let children: Vec<_> = child_ids.iter().map(|id| fbb.create_string(id)).collect();
    let children = fbb.create_vector(&children);
    let mask_ref = fbb.create_string(mask_id);
    let group = Group::create(
        &mut fbb,
        &GroupArgs {
            child_ids: Some(children),
            mask_child_id: Some(mask_ref),
            solo_child_id: None,
        },
    );
    let transform = Transform::create(&mut fbb, &TransformArgs::default());
    let id = fbb.create_string("group");
    let name = fbb.create_string("Group");
    let group = Node::create(
        &mut fbb,
        &NodeArgs {
            id: Some(id),
            name: Some(name),
            component_type: Component::Group,
            component: Some(group.as_union_value()),
            transform: Some(transform),
            visible: true,
            opacity: 1.0,
            ..Default::default()
        },
    );

    let outsider = rect_node(
        &mut fbb, "outsider", "Outsider", 0.0, 0.0, 20.0, 20.0, None, true,
    );

    let nodes = fbb.create_vector(&[group, mask, art, outsider]);
    wrap_document(&mut fbb, "Mask Fixture", nodes)
}

/// The editor stacks fills on one node — a solid under a gradient is how a
/// tint is built — and the runtime drew only the first.
#[test]
fn draws_every_stacked_fill_not_just_the_first() {
    let bytes = build_multi_fill_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    assert_eq!(
        scene.commands[0].fills,
        vec![
            RenderPaint::Solid(RenderColor { r: 255, g: 0, b: 0, a: 255 }),
            RenderPaint::Solid(RenderColor { r: 0, g: 0, b: 255, a: 255 }),
        ],
        "both fills should reach the scene, bottom-most first"
    );
}

/// `RenderStroke` was `{color, width}`, so cap, join, mitre limit and dashes
/// were read out of the file and thrown away.
#[test]
fn carries_the_authored_stroke_attributes() {
    let bytes = build_stroke_attributes_doc();
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    let stroke = scene.commands[0]
        .stroke
        .as_ref()
        .expect("the shape should stroke");
    assert_eq!(stroke.cap, RenderCap::Round);
    assert_eq!(stroke.join, RenderJoin::Bevel);
    assert_close(stroke.miter_limit, 2.5, 1e-6);
    assert_eq!(stroke.dash_array, vec![4.0, 2.0]);
    assert_close(stroke.dash_offset, 1.0, 1e-6);
    assert_eq!(stroke.align, RenderStrokeAlign::Outside);
    // Issue #158: the trim used to stop at the exporter; nothing drew it.
    assert_close(stroke.trim_start, 0.1, 1e-6);
    assert_close(stroke.trim_end, 0.6, 1e-6);
    assert_close(stroke.trim_offset, 0.25, 1e-6);
    assert!(stroke.is_trimmed());
}

fn build_multi_fill_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let red = solid_fill(&mut fbb, RenderColor { r: 255, g: 0, b: 0, a: 255 }, 1.0);
    let blue = solid_fill(&mut fbb, RenderColor { r: 0, g: 0, b: 255, a: 255 }, 1.0);
    let fills = fbb.create_vector(&[red, blue]);
    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 20.0,
            height: 20.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            fills_v2: Some(fills),
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "stacked",
        "Stacked",
        0.0,
        0.0,
        Component::Shape,
        shape.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);
    wrap_document(&mut fbb, "Multi Fill Fixture", nodes)
}

fn build_stroke_attributes_doc() -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let dashes = fbb.create_vector(&[4.0f32, 2.0]);
    let stroke_color = Color::new(10, 20, 30, 255);
    let stroke = Stroke::create(
        &mut fbb,
        &StrokeArgs {
            color: Some(&stroke_color),
            width: 3.0,
            opacity: 1.0,
            cap: StrokeCap::Round,
            join: StrokeJoin::Bevel,
            miter_limit: 2.5,
            align: StrokeAlign::Outside,
            dash_array: Some(dashes),
            dash_offset: 1.0,
            trim_start: 0.1,
            trim_end: 0.6,
            trim_offset: 0.25,
            ..Default::default()
        },
    );
    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 20.0,
            height: 20.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            stroke: Some(stroke),
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "dashed",
        "Dashed",
        0.0,
        0.0,
        Component::Shape,
        shape.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);
    wrap_document(&mut fbb, "Stroke Fixture", nodes)
}

/// A 2x1 PNG: opaque red, then blue at half alpha. Written out byte by byte so
/// the test carries no fixture file to drift away from it.
const TINY_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 244, 34, 127, 138, 0, 0, 0, 14, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 66, 13,
    0, 15, 122, 3, 126, 119, 233, 127, 151, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Image nodes shipped their PNG bytes in every package and the renderer drew
/// nothing at all with them.
#[test]
fn decodes_an_image_node_into_premultiplied_pixels() {
    let bytes = build_image_doc(TINY_PNG);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    match &scene.commands[0].shape {
        RenderShape::Image {
            width,
            height,
            rgba,
            pixel_width,
            pixel_height,
        } => {
            assert_close(*width, 64.0, 1e-9);
            assert_close(*height, 32.0, 1e-9);
            assert_eq!((*pixel_width, *pixel_height), (2, 1));
            // Red at full alpha survives. The blue pixel is (0,0,255) at
            // alpha 128, so its blue channel premultiplies to 255*128/255 =
            // 128; straight alpha would leave 255 there and fringe every soft
            // edge.
            assert_eq!(rgba.as_slice(), &[255, 0, 0, 255, 0, 0, 128, 128]);
        }
        other => panic!("expected an image, got {other:?}"),
    }
}

/// A broken asset should read as missing artwork, not as a blank rectangle
/// painted over whatever is behind it.
#[test]
fn an_undecodable_image_draws_nothing_rather_than_a_placeholder() {
    let bytes = build_image_doc(b"not a png");
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    assert!(build_render_scene(document, &frame).commands.is_empty());
}

fn build_image_doc(png: &[u8]) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let data = fbb.create_vector(png);
    let image = Image::create(
        &mut fbb,
        &ImageArgs {
            width: 64.0,
            height: 32.0,
            png_data: Some(data),
        },
    );
    let node = node(
        &mut fbb,
        "picture",
        "Picture",
        0.0,
        0.0,
        Component::Image,
        image.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);
    wrap_document(&mut fbb, "Image Fixture", nodes)
}

/// Text nodes exported and drew nothing at all, because the runtime had no
/// font to draw them with. The face travels with the document now.
#[test]
fn lays_text_out_into_glyphs_using_the_documents_own_font() {
    // The editor's own bundled Inter, which is what its exporter ships.
    let font = inter_regular();
    let bytes = build_text_doc(&font, "Hi", 32.0, TextAlign::Left);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    match &scene.commands[0].shape {
        RenderShape::Text {
            glyphs, font_size, ..
        } => {
            assert_close(*font_size, 32.0, 1e-9);
            assert_eq!(glyphs.len(), 2, "one glyph per character");
            // Laid out left to right: the second glyph sits an advance along,
            // on the same baseline.
            assert_close(glyphs[0].x, 0.0, 1e-9);
            assert!(
                glyphs[1].x > glyphs[0].x,
                "the second glyph should advance past the first"
            );
            assert_close(glyphs[1].y, glyphs[0].y, 1e-9);
            assert!(glyphs[0].y > 0.0, "the baseline sits below the box top");
            assert_ne!(glyphs[0].id, glyphs[1].id, "H and i are different glyphs");
        }
        other => panic!("expected text, got {other:?}"),
    }
}

/// Centring is measured against the node's authored box, which is what the
/// editor draws into.
#[test]
fn centres_text_in_the_nodes_own_box() {
    let font = inter_regular();
    let left = first_glyph_x(&build_text_doc(&font, "Hi", 32.0, TextAlign::Left));
    let centred = first_glyph_x(&build_text_doc(&font, "Hi", 32.0, TextAlign::Center));
    let right = first_glyph_x(&build_text_doc(&font, "Hi", 32.0, TextAlign::Right));

    assert_close(left, 0.0, 1e-9);
    assert!(centred > left, "centred text starts further in than left");
    assert!(right > centred, "right-aligned starts further in still");
}

/// The runtime has no system-font lookup on purpose: substituting a face would
/// render the document in the wrong one and look like a rendering bug.
#[test]
fn text_with_no_font_in_the_document_draws_nothing() {
    let bytes = build_text_doc(&[], "Hi", 32.0, TextAlign::Left);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    assert!(build_render_scene(document, &frame).commands.is_empty());
}

/// Inter Regular, the face the Kineto exporter ships (SIL OFL 1.1, see
/// tests/fixtures/fonts/OFL.txt). Resolved from the crate root so the test
/// does not depend on the working directory.
fn inter_regular() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fonts/Inter-Regular.ttf");
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("Inter Regular test fixture missing at {path:?}: {e}"))
}

fn first_glyph_x(bytes: &[u8]) -> f64 {
    let document = format::load(bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.to_vec()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");
    match &build_render_scene(document, &frame).commands[0].shape {
        RenderShape::Text { glyphs, .. } => glyphs[0].x,
        other => panic!("expected text, got {other:?}"),
    }
}

fn build_text_doc(font: &[u8], content: &str, size: f32, align: TextAlign) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(1 << 20);

    let content = fbb.create_string(content);
    let family = fbb.create_string("Inter");
    let colour = Color::new(255, 255, 255, 255);
    let text = Text::create(
        &mut fbb,
        &TextArgs {
            content: Some(content),
            font_family: Some(family),
            font_size: size,
            font_weight: 400,
            color: Some(&colour),
            align,
            width: 200.0,
            height: 40.0,
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "label",
        "Label",
        0.0,
        0.0,
        Component::Text,
        text.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);

    let faces = if font.is_empty() {
        None
    } else {
        let family = fbb.create_string("Inter");
        let data = fbb.create_vector(font);
        let face = FontFace::create(
            &mut fbb,
            &FontFaceArgs {
                family: Some(family),
                weight: 400,
                italic: false,
                data: Some(data),
            },
        );
        Some(fbb.create_vector(&[face]))
    };

    let name = fbb.create_string("Text Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 100.0,
            nodes: Some(nodes),
            fonts: faces,
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

/// The runtime drew in document order and could not re-sort, so a node sent
/// forward or back in the editor did not move on device.
#[test]
fn draws_in_the_authored_z_order_not_document_order() {
    let bytes = build_draw_order_doc(&[("back", 10), ("front", -5), ("middle", 0)]);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    assert_eq!(
        scene
            .commands
            .iter()
            .map(|c| c.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["front", "middle", "back"],
    );
}

/// Every pre-#111 file says 0 for every node, so the sort must leave document
/// order exactly as it found it.
#[test]
fn a_document_with_no_authored_order_keeps_document_order() {
    let bytes = build_draw_order_doc(&[("c", 0), ("a", 0), ("b", 0)]);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    assert_eq!(
        build_render_scene(document, &frame)
            .commands
            .iter()
            .map(|c| c.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["c", "a", "b"],
    );
}

fn build_draw_order_doc(nodes: &[(&'static str, i32)]) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);
    let built: Vec<_> = nodes
        .iter()
        .map(|(id, order)| {
            let rect = Rectangle::create(
                &mut fbb,
                &RectangleArgs {
                    width: 10.0,
                    height: 10.0,
                    ..Default::default()
                },
            );
            let shape = Shape::create(
                &mut fbb,
                &ShapeArgs {
                    geometry_type: Geometry::Rectangle,
                    geometry: Some(rect.as_union_value()),
                    ..Default::default()
                },
            );
            let transform = Transform::create(&mut fbb, &TransformArgs::default());
            let node_id = fbb.create_string(id);
            let name = fbb.create_string(id);
            Node::create(
                &mut fbb,
                &NodeArgs {
                    id: Some(node_id),
                    name: Some(name),
                    component_type: Component::Shape,
                    component: Some(shape.as_union_value()),
                    transform: Some(transform),
                    visible: true,
                    opacity: 1.0,
                    draw_order: *order,
                    ..Default::default()
                },
            )
        })
        .collect();
    let nodes = fbb.create_vector(&built);
    wrap_document(&mut fbb, "Draw Order Fixture", nodes)
}

/// `gradientStop*` used to export as `FillColor`/`Opacity` — the "closest
/// analogue" — so animating a stop replaced the node's whole fill or faded the
/// node. They have kinds of their own now.
#[test]
fn gradient_stop_tracks_move_the_gradient_rather_than_the_node() {
    let bytes = build_gradient_doc(false);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let mut frame = player.play().expect("frame should evaluate");
    {
        let values = frame
            .node_overrides
            .entry("gradient".to_string())
            .or_default();
        values
            .values
            .insert("gradientStopOffset", PropertyValue::Scalar(0.25));
        values
            .values
            .insert("gradientStopOpacity", PropertyValue::Scalar(0.5));
    }

    let scene = build_render_scene(document, &frame);

    match scene.commands[0].fills.first().expect("still a gradient") {
        RenderPaint::LinearGradient { stops, .. } => {
            // Every stop shifts by the same amount, so the gradient slides
            // rather than collapsing onto one colour.
            assert_close(stops[0].offset, 0.25, 1e-6);
            assert_close(stops[1].offset, 1.0, 1e-6);
            assert_eq!(stops[0].color.a, 128, "the opacity track fades the stops");
        }
        other => panic!("expected a linear gradient, got {other:?}"),
    }
}

/// `fontSize`, `letterSpacing` and `lineHeight` have no static field in the
/// editor's model for the latter two, so the track is the value.
#[test]
fn text_layout_follows_its_animation_tracks() {
    let font = inter_regular();
    let bytes = build_text_doc(&font, "Hi\nHo", 32.0, TextAlign::Left);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let mut frame = player.play().expect("frame should evaluate");
    {
        let values = frame.node_overrides.entry("label".to_string()).or_default();
        values
            .values
            .insert("letterSpacing", PropertyValue::Scalar(10.0));
        values
            .values
            .insert("lineHeight", PropertyValue::Scalar(2.0));
    }

    let scene = build_render_scene(document, &frame);
    let RenderShape::Text { glyphs, .. } = &scene.commands[0].shape else {
        panic!("expected text");
    };

    let plain = {
        let document = format::load(&bytes).expect("document should parse");
        let mut player = KinetoPlayer::load(bytes.clone()).expect("player");
        let frame = player.play().expect("frame");
        match &build_render_scene(document, &frame).commands[0].shape {
            RenderShape::Text { glyphs, .. } => glyphs.clone(),
            other => panic!("expected text, got {other:?}"),
        }
    };

    assert_close(glyphs[1].x - plain[1].x, 10.0, 1e-6);
    // Line height is a multiplier of the font size, so 2.0 puts the second
    // line 64 units below the first.
    assert_close(glyphs[2].y - glyphs[0].y, 64.0, 1e-6);
}

/// The editor has drawn through a virtual camera since M11 and the runtime had
/// none, so a document that panned or zoomed sat still on device. The tracks
/// were doubly dead: they target the reserved `__camera__` id, which matches no
/// node, and the runtime looked them up as a node.
#[test]
fn the_document_is_drawn_through_its_camera() {
    let bytes = build_camera_doc(Some((30.0, 0.0, 2.0, 0.0)));
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);

    // Zoom 2 about the canvas centre (100, 50), then pan by 30.
    assert_close(scene.camera[0], 2.0, 1e-9);
    assert_close(scene.camera[3], 2.0, 1e-9);
    assert_close(scene.camera[4], 100.0 - 2.0 * (100.0 - 30.0), 1e-9);
    assert_close(scene.camera[5], 50.0 - 2.0 * 50.0, 1e-9);
}

#[test]
fn a_document_with_no_camera_is_drawn_untransformed() {
    let bytes = build_camera_doc(None);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    assert_eq!(
        build_render_scene(document, &frame).camera,
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
    );
}

/// A camera track drives the camera, not a node — the reserved subject id is
/// the whole point.
#[test]
fn a_camera_track_moves_the_camera() {
    let bytes = build_camera_doc(None);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let mut frame = player.play().expect("frame should evaluate");
    frame
        .node_overrides
        .entry("__camera__".to_string())
        .or_default()
        .values
        .insert("cameraZoom", PropertyValue::Scalar(3.0));

    let scene = build_render_scene(document, &frame);

    assert_close(scene.camera[0], 3.0, 1e-9);
    // And no node was moved by it.
    assert_close(scene.commands[0].transform.x, 0.0, 1e-9);
}

fn build_camera_doc(camera: Option<(f32, f32, f32, f32)>) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 10.0,
            height: 10.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "box",
        "Box",
        0.0,
        0.0,
        Component::Shape,
        shape.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);

    let camera = camera.map(|(x, y, zoom, rotation)| {
        Camera::create(
            &mut fbb,
            &CameraArgs {
                x,
                y,
                zoom,
                rotation,
            },
        )
    });

    let name = fbb.create_string("Camera Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 100.0,
            nodes: Some(nodes),
            camera,
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}

/// `RenderCommand` had no effects member at all, so a shape with a drop shadow
/// drew flat on device.
#[test]
fn carries_drop_shadows_into_the_scene() {
    let bytes = build_shadow_doc(true, false);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);
    let shadow = scene.commands[0]
        .shadows
        .first()
        .expect("the shape should carry its shadow");

    assert_close(shadow.offset_x, 6.0, 1e-6);
    assert_close(shadow.offset_y, 8.0, 1e-6);
    assert_close(shadow.blur, 10.0, 1e-6);
    // The effect's own opacity multiplies into the colour, the way the editor
    // composes it.
    assert_eq!(shadow.color.a, 128);
}

#[test]
fn a_disabled_shadow_is_not_drawn() {
    let bytes = build_shadow_doc(false, false);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    assert!(build_render_scene(document, &frame).commands[0].shadows.is_empty());
}

/// An inset shadow darkens the shape's interior, which the flat command model
/// has nowhere to express. Drawing it as an ordinary shadow would put the
/// darkening on the wrong side, so it is left out.
#[test]
fn an_inset_shadow_is_left_out_rather_than_drawn_outside() {
    let bytes = build_shadow_doc(true, true);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.play().expect("frame should evaluate");

    assert!(build_render_scene(document, &frame).commands[0].shadows.is_empty());
}

fn build_shadow_doc(enabled: bool, inset: bool) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(4096);
    let colour = Color::new(0, 0, 0, 255);
    let shadow = DropShadow::create(
        &mut fbb,
        &DropShadowArgs {
            color: Some(&colour),
            opacity: 0.5,
            offset_x: 6.0,
            offset_y: 8.0,
            blur: 10.0,
            spread: 0.0,
            inset,
            enabled,
        },
    );
    let entry = EffectEntry::create(
        &mut fbb,
        &EffectEntryArgs {
            kind_type: EffectKind::DropShadow,
            kind: Some(shadow.as_union_value()),
        },
    );
    let effects = fbb.create_vector(&[entry]);

    let rect = Rectangle::create(
        &mut fbb,
        &RectangleArgs {
            width: 40.0,
            height: 40.0,
            ..Default::default()
        },
    );
    let shape = Shape::create(
        &mut fbb,
        &ShapeArgs {
            geometry_type: Geometry::Rectangle,
            geometry: Some(rect.as_union_value()),
            effects: Some(effects),
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "card",
        "Card",
        0.0,
        0.0,
        Component::Shape,
        shape.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);
    wrap_document(&mut fbb, "Shadow Fixture", nodes)
}

/// A `pathData` morph exported as a `PositionX` of 0.0 — the "closest
/// analogue" — so a morphing shape teleported to x=0 instead of changing
/// shape. It has a kind of its own now, and the vertices travel with it.
#[test]
fn a_path_morph_replaces_the_vertices() {
    let bytes = build_morph_doc(3);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");

    // Half way between the authored triangle and the morph target.
    let frame = player.seek(0.5).expect("frame should evaluate");
    let scene = build_render_scene(document, &frame);

    let RenderShape::Path { contours, .. } = &scene.commands[0].shape else {
        panic!("expected a path");
    };
    let anchors: Vec<(f64, f64)> = contours[0]
        .points
        .iter()
        .map(|p| (p.anchor_x, p.anchor_y))
        .collect();

    // Authored (0,0) → morph (100,0), so half way is (50,0).
    assert_close(anchors[0].0, 50.0, 1e-6);
    assert_close(anchors[0].1, 0.0, 1e-6);
    // And the node itself did not move.
    assert_close(scene.commands[0].transform.x, 0.0, 1e-6);
}

/// A morph recorded against a different shape has no correspondence to apply,
/// so the authored geometry holds — the same rule the editor uses.
#[test]
fn a_morph_with_the_wrong_vertex_count_holds_the_authored_shape() {
    let bytes = build_morph_doc(2);
    let document = format::load(&bytes).expect("document should parse");
    let mut player = KinetoPlayer::load(bytes.clone()).expect("player should load");
    let frame = player.seek(0.5).expect("frame should evaluate");

    let scene = build_render_scene(document, &frame);
    let RenderShape::Path { contours, .. } = &scene.commands[0].shape else {
        panic!("expected a path");
    };
    assert_close(contours[0].points[0].anchor_x, 0.0, 1e-6);
}

/// The morph's vertex count, so the mismatch case can be built too.
fn build_morph_doc(morph_vertices: usize) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::with_capacity(8192);

    let authored: Vec<_> = [(0.0f32, 0.0f32), (40.0, 0.0), (40.0, 40.0)]
        .into_iter()
        .map(|(x, y)| path_point(&mut fbb, x, y))
        .collect();
    let authored = fbb.create_vector(&authored);
    let path = Path::create(
        &mut fbb,
        &PathArgs {
            nodes_v2: Some(authored),
            closed: true,
            ..Default::default()
        },
    );
    let node = node(
        &mut fbb,
        "blob",
        "Blob",
        0.0,
        0.0,
        Component::Path,
        path.as_union_value(),
        true,
    );
    let nodes = fbb.create_vector(&[node]);

    // Two keyframes: the authored shape at t=0, the target at t=1.
    let mut keyframes = Vec::new();
    for (index, (time, shift)) in [(0.0f32, 0.0f32), (1.0, 100.0)].into_iter().enumerate() {
        let verts: Vec<_> = (0..morph_vertices)
            .map(|i| {
                let base = [(0.0f32, 0.0f32), (40.0, 0.0), (40.0, 40.0)][i.min(2)];
                path_point(&mut fbb, base.0 + shift, base.1)
            })
            .collect();
        let verts = fbb.create_vector(&verts);
        let kf_id = fbb.create_string(if index == 0 { "kf0" } else { "kf1" });
        keyframes.push(KFKeyframe::create(
            &mut fbb,
            &KFKeyframeArgs {
                id: Some(kf_id),
                time,
                path_vertices: Some(verts),
                ..Default::default()
            },
        ));
    }
    let keyframes = fbb.create_vector(&keyframes);
    let track_id = fbb.create_string("track");
    let target = fbb.create_string("blob");
    let track = KFTrack::create(
        &mut fbb,
        &KFTrackArgs {
            id: Some(track_id),
            target_node_id: Some(target),
            property: AnimatedPropertyKind::PathData,
            keyframes: Some(keyframes),
            ..Default::default()
        },
    );
    let tracks = fbb.create_vector(&[track]);
    let clip_id = fbb.create_string("clip");
    let clip_name = fbb.create_string("Morph");
    let animation = Animation::create(
        &mut fbb,
        &AnimationArgs {
            id: Some(clip_id),
            name: Some(clip_name),
            duration: 1.0,
            tracks: Some(tracks),
            ..Default::default()
        },
    );
    let animations = fbb.create_vector(&[animation]);

    let name = fbb.create_string("Morph Fixture");
    let document = Document::create(
        &mut fbb,
        &DocumentArgs {
            name: Some(name),
            canvas_width: 200.0,
            canvas_height: 200.0,
            nodes: Some(nodes),
            animations: Some(animations),
            ..Default::default()
        },
    );
    finish_document_buffer(&mut fbb, document);
    fbb.finished_data().to_vec()
}
