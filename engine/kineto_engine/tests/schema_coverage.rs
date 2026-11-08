//! The runtime half of the parity guardrail (issue #111).
//!
//! The editor half is `lib/src/data/runtime_capabilities.dart`, where adding a
//! capability is a *compile* error until someone classifies it. FlatBuffers
//! emits schema enums as newtype structs rather than Rust enums, so the same
//! trick is unavailable here — `match` cannot be exhaustive over a `u8`
//! wrapper, and every such match needs a `_` arm that would swallow a new
//! variant in silence.
//!
//! These tests are the stand-in. They walk the `ENUM_VALUES_*` arrays, which
//! the FlatBuffers generator rebuilds from `schema/kineto_runtime.fbs` on
//! every build, so they grow the moment the schema does. Adding a variant
//! without deciding what the runtime does with it fails here.
//!
//! This is what nobody had when `AnimatedPropertyKind` stopped keeping up with
//! the editor and a document rendered wrong on a device with nothing to point
//! at.

use kineto_engine::animation::timeline_eval::{property_key, UNHANDLED_PROPERTY};
use kineto_engine::format::generated::kineto::{
    Component, ENUM_VALUES_ANIMATED_PROPERTY_KIND, ENUM_VALUES_BLEND_MODE, ENUM_VALUES_COMPONENT,
};
use kineto_engine::render::vello_scene::{
    blend_mode_from_kineto, component_support, ComponentSupport,
};

#[test]
fn every_animated_property_kind_in_the_schema_has_an_override_key() {
    let unhandled: Vec<String> = ENUM_VALUES_ANIMATED_PROPERTY_KIND
        .iter()
        .filter(|kind| property_key(**kind) == UNHANDLED_PROPERTY)
        .map(|kind| format!("{kind:?}"))
        .collect();

    assert!(
        unhandled.is_empty(),
        "these AnimatedPropertyKinds are in the schema and the timeline \
         evaluator does not know them, so tracks using them are silently \
         dropped: {unhandled:?}. Add an arm to `property_key`, and add the \
         matching verdict to `RuntimeCapabilities.animatedProperty`."
    );
}

#[test]
fn every_component_in_the_schema_has_a_stated_verdict() {
    let unclassified: Vec<String> = ENUM_VALUES_COMPONENT
        .iter()
        .filter(|component| component_support(**component) == ComponentSupport::Unclassified)
        .map(|component| format!("{component:?}"))
        .collect();

    assert!(
        unclassified.is_empty(),
        "these components are in the schema and nothing says whether the \
         runtime draws them: {unclassified:?}. Either draw them in \
         `build_render_scene`, or mark them `NotDrawn` with the reason an \
         author would need to hear."
    );
}

#[test]
fn a_component_marked_drawn_is_reachable_from_the_renderer() {
    // Guards the other direction: `component_support` claiming `Drawn` for
    // something the dispatch does not actually handle would make the test
    // above pass while the node stayed invisible.
    for component in ENUM_VALUES_COMPONENT {
        if component_support(component) != ComponentSupport::Drawn {
            continue;
        }
        assert!(
            matches!(
                component,
                Component::Shape
                    | Component::Path
                    | Component::CompoundPath
                    | Component::Image
                    | Component::Text
            ),
            "{component:?} is marked Drawn but `build_render_scene` has no arm \
             that emits a command for it"
        );
    }
}

#[test]
fn every_blend_mode_in_the_schema_reaches_the_renderer() {
    let unhandled: Vec<String> = ENUM_VALUES_BLEND_MODE
        .iter()
        .filter(|mode| blend_mode_from_kineto(**mode).is_none())
        .map(|mode| format!("{mode:?}"))
        .collect();

    assert!(
        unhandled.is_empty(),
        "these blend modes are in the schema and the renderer would flatten \
         them to Normal: {unhandled:?}. Add an arm to `blend_mode_from_kineto` \
         and a verdict to `RuntimeCapabilities.blendMode`."
    );
}
