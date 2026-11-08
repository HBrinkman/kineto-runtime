use std::collections::{HashMap, HashSet};

use crate::animation::blend::{blend_ordered, mix_samples, LayerSample};
use crate::animation::{evaluate_animation, static_property_key, NodeOverrides, PropertyValue};
use crate::bindings::apply_bindings;
use crate::bones::{
    bones_from_document, evaluate_skeletal, ik_chains_from_document, ik_targets_from_document,
    Matrix4, RuntimeIkTarget,
};
use crate::format;
use crate::hierarchy::{apply_point, group_carry_deltas, invert, IDENTITY};
use crate::state_machine::blend_states::weights_for_state;
use crate::state_machine::{
    layers_of, LayerGraph, SmEvalState, SmEvaluator, SmInputValues, TransitionManager,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TapTarget {
    pub node_id: String,
    pub node_name: String,
}

enum ListenerWrite {
    Bool(String, bool),
    /// Input id plus its authored default, captured while the document is in
    /// scope. The flip itself happens at apply time against the live value —
    /// two toggles on one input in one event must compound, not cancel.
    ToggleBool(String, bool),
    Number(String, f64),
    Trigger(String),
}

#[derive(Debug)]
pub struct EvaluatedFrame {
    pub current_time: f64,
    pub active_animation_id: Option<String>,
    pub node_overrides: HashMap<String, NodeOverrides>,
    pub fk: HashMap<String, Matrix4>,
    /// Targets in the pose this frame put them in — keyframes applied, group
    /// carried. Hit-testing a region reads these rather than the stored
    /// positions, so a region moves with what it belongs to (issue #74).
    pub ik_targets: Vec<RuntimeIkTarget>,
}

#[derive(Debug, Clone)]
struct MachineRuntimeState {
    /// The base layer's evaluation state, which is also what the pre-layers
    /// API (`active_state`, `active_transition`, `transition_progress`)
    /// reports.
    eval_state: SmEvalState,
    /// Every other layer's own state, keyed by layer id (issue #98).
    ///
    /// Each carries its own elapsed seconds, so a Face layer sitting in a
    /// two-second state and a Mouth layer flicking between short ones keep
    /// independent clocks — the whole point of layers being independent
    /// systems rather than one graph.
    layer_states: HashMap<String, SmEvalState>,
    input_values: SmInputValues,
}

impl MachineRuntimeState {
    fn new() -> Self {
        Self {
            eval_state: SmEvalState::default(),
            layer_states: HashMap::new(),
            input_values: SmInputValues::default(),
        }
    }

    fn state_of(&self, graph: &LayerGraph<'_>) -> SmEvalState {
        if graph.is_base() {
            self.eval_state.clone()
        } else {
            self.layer_states
                .get(&graph.id())
                .cloned()
                .unwrap_or_default()
        }
    }

    fn set_state_of(&mut self, graph: &LayerGraph<'_>, state: SmEvalState) {
        if graph.is_base() {
            self.eval_state = state;
        } else {
            self.layer_states.insert(graph.id(), state);
        }
    }
}

/// Rust-only runtime player for M7.7.
///
/// The player owns `.kvk` bytes and reparses the FlatBuffer zero-copy for each
/// tick. That keeps the API simple while the FFI/rendering layers are still
/// under construction.
#[derive(Debug, Clone)]
pub struct KinetoPlayer {
    bytes: Vec<u8>,
    active_animation_id: Option<String>,
    /// Where each pointer-following target is being asked to go, and how fast
    /// it is currently travelling. Empty until a pointer event names one.
    follow_desired: HashMap<String, (f64, f64)>,
    follow_position: HashMap<String, (f64, f64)>,
    follow_velocity: HashMap<String, (f64, f64)>,
    current_time: f64,
    /// Ping-pong playback direction: `1` forward, `-1` backward. Meaningless
    /// for other loop modes. Tracked explicitly rather than re-derived from
    /// `current_time` each tick — deriving it from the bounced time alone
    /// loses cycle parity and freezes the clip at its end under real
    /// per-frame ticking (issue #43).
    direction: i8,
    play_state: PlayState,
    machines: HashMap<String, MachineRuntimeState>,
    /// Sounds fired since the host last collected them (issue #111).
    ///
    /// The engine has no audio device and a `.kvk` carries no sound files, so
    /// firing a sound means *telling the host*. Buffered rather than returned
    /// from `advance`, because a listener fires one outside any tick.
    fired_audio_event_ids: Vec<String>,
}

impl KinetoPlayer {
    pub fn load(bytes: impl Into<Vec<u8>>) -> Result<Self, format::InvalidFlatbuffer> {
        let bytes = bytes.into();
        let document = format::load(&bytes)?;
        let mut machines = HashMap::new();
        for machine in document.state_machines().unwrap_or_default() {
            machines.insert(machine.id().to_owned(), MachineRuntimeState::new());
        }

        Ok(Self {
            bytes,
            fired_audio_event_ids: Vec::new(),
            active_animation_id: None,
            follow_desired: HashMap::new(),
            follow_position: HashMap::new(),
            follow_velocity: HashMap::new(),
            current_time: 0.0,
            direction: 1,
            play_state: PlayState::Stopped,
            machines,
        })
    }

    pub fn current_time(&self) -> f64 {
        self.current_time
    }

    pub fn play_state(&self) -> PlayState {
        self.play_state
    }

    pub fn play(&mut self) -> Result<EvaluatedFrame, format::InvalidFlatbuffer> {
        if self.active_animation_id.is_none() {
            let doc = self.document()?;
            let animations = doc.animations().unwrap_or_default();
            if !animations.is_empty() {
                self.active_animation_id = Some(animations.get(0).id().to_owned());
            }
        }
        self.play_state = PlayState::Playing;
        self.evaluate_frame()
    }

    pub fn pause(&mut self) {
        self.play_state = PlayState::Paused;
    }

    pub fn stop(&mut self) -> Result<EvaluatedFrame, format::InvalidFlatbuffer> {
        self.play_state = PlayState::Stopped;
        self.current_time = 0.0;
        self.direction = 1;
        self.evaluate_frame()
    }

    pub fn seek(&mut self, time: f64) -> Result<EvaluatedFrame, format::InvalidFlatbuffer> {
        self.current_time = time.max(0.0);
        // Scrubbing always resumes forward — see Dart Playback.seek.
        self.direction = 1;
        self.evaluate_frame()
    }

    pub fn select_animation(
        &mut self,
        animation_id: &str,
    ) -> Result<(), format::InvalidFlatbuffer> {
        let doc = self.document()?;
        if doc
            .animations()
            .unwrap_or_default()
            .iter()
            .any(|animation| animation.id() == animation_id)
        {
            self.active_animation_id = Some(animation_id.to_owned());
            self.current_time = 0.0;
            self.direction = 1;
        }
        Ok(())
    }

    pub fn advance(&mut self, dt: f64) -> Result<EvaluatedFrame, format::InvalidFlatbuffer> {
        let delta = dt.max(0.0);
        self.chase_following_targets(delta);
        self.advance_state_machines(delta)?;

        if self.play_state == PlayState::Playing {
            if let Some((duration, loop_mode)) = self.active_animation_timing()? {
                match loop_mode {
                    format::LoopMode::PingPong => self.advance_ping_pong(delta, duration),
                    _ => {
                        self.current_time += delta;
                        self.clamp_or_loop(duration, loop_mode);
                    }
                }
            }
        }

        self.evaluate_frame()
    }

    pub fn set_bool(
        &mut self,
        state_machine_id: &str,
        input_name: &str,
        value: bool,
    ) -> Result<(), String> {
        let input_id = self.find_input_id(state_machine_id, input_name, format::InputKind::Bool)?;
        let machine = self
            .machines
            .get_mut(state_machine_id)
            .ok_or_else(|| format!("Unknown state machine id \"{state_machine_id}\""))?;
        machine.input_values = machine.input_values.clone().set_bool(input_id, value);
        Ok(())
    }

    /// Sets a number input to a normalised `0..1` value.
    ///
    /// Out-of-range values are kept as given and clamped where they are read,
    /// so a host that feeds raw pointer coordinates it has not normalised yet
    /// gets a pinned property rather than a rig flung off-canvas.
    pub fn set_number(
        &mut self,
        state_machine_id: &str,
        input_name: &str,
        value: f64,
    ) -> Result<(), String> {
        let input_id =
            self.find_input_id(state_machine_id, input_name, format::InputKind::Number)?;
        let machine = self
            .machines
            .get_mut(state_machine_id)
            .ok_or_else(|| format!("Unknown state machine id \"{state_machine_id}\""))?;
        machine.input_values = machine.input_values.clone().set_number(input_id, value);
        Ok(())
    }

    pub fn set_trigger(&mut self, state_machine_id: &str, input_name: &str) -> Result<(), String> {
        let input_id =
            self.find_input_id(state_machine_id, input_name, format::InputKind::Trigger)?;
        let machine = self
            .machines
            .get_mut(state_machine_id)
            .ok_or_else(|| format!("Unknown state machine id \"{state_machine_id}\""))?;
        machine.input_values = machine.input_values.clone().queue_trigger(input_id);
        Ok(())
    }

    pub fn active_state(&self, state_machine_id: &str) -> Option<&str> {
        self.machines
            .get(state_machine_id)
            .and_then(|machine| machine.eval_state.active_state_id.as_deref())
    }

    pub fn active_transition(&self, state_machine_id: &str) -> Option<&str> {
        self.machines
            .get(state_machine_id)
            .and_then(|machine| machine.eval_state.active_transition_id.as_deref())
    }

    pub fn transition_progress(&self, state_machine_id: &str) -> Option<f64> {
        let machine_state = self.machines.get(state_machine_id)?;
        let transition_id = machine_state.eval_state.active_transition_id.as_deref()?;
        let document = self.document().ok()?;
        let machine = document
            .state_machines()
            .unwrap_or_default()
            .iter()
            .find(|machine| machine.id() == state_machine_id)?;
        let transition = machine
            .transitions()
            .unwrap_or_default()
            .iter()
            .find(|transition| transition.id() == transition_id)?;
        Some(TransitionManager::progress(
            &machine_state.eval_state,
            transition,
        ))
    }

    /// The interactive target under a point, if any.
    ///
    /// Targets are checked before artwork: a hit area is deliberately drawn
    /// over the thing it covers, and it is usually larger, so answering with
    /// the drawing underneath would make the region useless (issue #74).
    pub fn hit_test_targets(
        &self,
        x: f64,
        y: f64,
    ) -> Result<Option<String>, format::InvalidFlatbuffer> {
        // Authored positions, matching the editor
        // (`preview_pointer_drive.dart:193`) and the travel box
        // `apply_bindings` maps into. The evaluated position is where the
        // target currently sits *inside* that box — reading it here made a
        // bound target untouchable, because with its input at rest the target
        // sits against one edge and the box appeared to move with it.
        let document = self.document()?;
        for target in document.ik_targets().unwrap_or_default().iter() {
            if !target.interactive() {
                continue;
            }
            let (half_w, half_h) = (
                target.extent_w() as f64 / 2.0,
                target.extent_h() as f64 / 2.0,
            );
            if half_w <= 0.0 || half_h <= 0.0 {
                continue;
            }
            let (cx, cy) = (target.position_x() as f64, target.position_y() as f64);
            if (x - cx).abs() <= half_w && (y - cy).abs() <= half_h {
                return Ok(Some(target.id().to_owned()));
            }
        }
        Ok(None)
    }

    /// Delivers a pointer event at a document point to whatever listens there.
    ///
    /// This is the whole point of listeners: a tap drives an input without the
    /// host knowing which input, or that there is a state machine at all.
    /// Returns the ids of the listeners that fired.
    pub fn pointer_event(
        &mut self,
        event: format::ListenerEventKind,
        x: f64,
        y: f64,
    ) -> Result<Vec<String>, format::InvalidFlatbuffer> {
        let subject = match self.hit_test_targets(x, y)? {
            Some(target_id) => Some(target_id),
            None => self.hit_test_document_point(x, y)?.map(|hit| hit.node_id),
        };
        // Pointer-driven number inputs take the cursor's normalised position,
        // whether or not it landed on anything. The editor drives them from
        // the canvas box (`preview_pointer_drive.dart:255-266`); the runtime
        // had no source field at all, so a document with a pointer-driven
        // input sat still unless the host app knew to feed it (issue #111).
        self.drive_pointer_inputs(x, y)?;
        self.drive_target_extent_bindings(x, y)?;

        // A lifted or cancelled finger is "the pointer is no longer down":
        // every following target reverts to its authored position at once
        // rather than easing back, because `TargetFollowScope` has no reading
        // for "keep chasing the last spot" (`preview_pointer_drive.dart:124`).
        //
        // Holding the last position instead is what made the animation appear
        // to freeze after a tap: the follow wins over the keyframes, so a
        // target left parked there never moved again (issue #111).
        if event == format::ListenerEventKind::PointerUp
            || event == format::ListenerEventKind::PointerExit
        {
            self.follow_desired.clear();
            self.follow_position.clear();
            self.follow_velocity.clear();
        } else {
            self.aim_following_targets(x, y)?;
        }

        let Some(subject) = subject else {
            return Ok(Vec::new());
        };

        let document = self.document()?;
        let mut fired = Vec::new();
        let mut writes: Vec<(String, ListenerWrite)> = Vec::new();
        let mut audio: Vec<String> = Vec::new();

        for machine in document.state_machines().unwrap_or_default() {
            for listener in machine.listeners().unwrap_or_default() {
                if listener.subject_id() != subject || listener.event() != event {
                    continue;
                }
                fired.push(listener.id().to_owned());

                // A `PlayAudio` listener writes no input at all (issue #111):
                // it names a sound, which is reported to the host rather than
                // played here.
                if listener.action() == format::ListenerActionKind::PlayAudio {
                    if let Some(event_id) = listener.audio_event_id() {
                        audio.push(event_id.to_owned());
                    }
                    continue;
                }

                // Read the action first, then the input: since issue #111
                // `input_id` is optional in the schema, and a listener that
                // names none is malformed rather than meaningful.
                let Some(input_id) = listener.input_id() else {
                    continue;
                };

                writes.push((
                    machine.id().to_owned(),
                    match listener.action() {
                        format::ListenerActionKind::SetBoolean => {
                            ListenerWrite::Bool(input_id.to_owned(), listener.bool_value())
                        }
                        format::ListenerActionKind::SetNumber => {
                            ListenerWrite::Number(input_id.to_owned(), listener.number_value() as f64)
                        }
                        format::ListenerActionKind::ToggleBoolean => ListenerWrite::ToggleBool(
                            input_id.to_owned(),
                            find_machine_input(machine, input_id)
                                .map(|input| input.default_bool_value())
                                .unwrap_or(false),
                        ),
                        format::ListenerActionKind::FireTrigger => {
                            ListenerWrite::Trigger(input_id.to_owned())
                        }
                        _ => continue,
                    },
                ));
            }
        }

        for (machine_id, write) in writes {
            let machine = self
                .machines
                .entry(machine_id)
                .or_insert_with(MachineRuntimeState::new);
            // Listeners address inputs by id, unlike the host API, which
            // addresses them by name: the ids are what the document stores.
            machine.input_values = match write {
                ListenerWrite::Bool(input_id, value) => {
                    machine.input_values.clone().set_bool(input_id, value)
                }
                ListenerWrite::ToggleBool(input_id, default_value) => {
                    let was = machine.input_values.bool_value(&input_id, default_value);
                    machine.input_values.clone().set_bool(input_id, !was)
                }
                ListenerWrite::Number(input_id, value) => {
                    machine.input_values.clone().set_number(input_id, value)
                }
                ListenerWrite::Trigger(input_id) => {
                    machine.input_values.clone().queue_trigger(input_id)
                }
            };
        }

        self.fired_audio_event_ids.extend(audio);
        Ok(fired)
    }

    /// Takes the sounds fired since the last call, in the order they fired
    /// (issue #111).
    ///
    /// Draining, not peeking: each firing must reach the host exactly once,
    /// and a host that polls every frame would otherwise replay the whole
    /// history. Never de-duplicated — the same sound twice is two firings,
    /// which is what a double-tap is.
    pub fn take_fired_audio_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.fired_audio_event_ids)
    }

    /// What the timeline's audio should be doing right now (issue #111).
    ///
    /// One entry per audio clip that is audible at the current playhead, as
    /// `(track_id, asset_id, file_name, source_position_seconds, volume)`.
    /// `source_position` is the point *inside the file* that should be
    /// sounding, so a host can start a player there and keep it in step.
    ///
    /// The engine plays nothing and a `.kvk` carries no audio bytes: this is
    /// the runtime saying which file, where, and how loud, and the host
    /// deciding what to do about it.
    ///
    /// The timing rule is the editor's, unchanged: a clip occupies
    /// `[start_time, start_time + duration)`; `offset` is the in-point inside
    /// the asset; material past the end of the asset is silence rather than a
    /// stretched tail; a looping clip repeats `[offset, asset_duration)`.
    pub fn active_audio_clips(
        &self,
    ) -> Result<Vec<(String, String, String, f64, f64)>, format::InvalidFlatbuffer> {
        let Some(clip_id) = self.active_animation_id.clone() else {
            return Ok(Vec::new());
        };
        let document = self.document()?;
        let time = self.current_time;

        let assets: HashMap<&str, (&str, f64)> = document
            .audio_assets()
            .unwrap_or_default()
            .iter()
            .map(|asset| {
                (
                    asset.id(),
                    (asset.name(), asset.duration() as f64),
                )
            })
            .collect();

        let mut audible = Vec::new();
        for track in document.audio_tracks().unwrap_or_default() {
            if track.mute() || track.clip_id() != clip_id {
                continue;
            }
            let Some((file_name, asset_duration)) = assets.get(track.asset_id()) else {
                // A clip whose asset never travelled: nothing to name, so
                // nothing to say.
                continue;
            };

            let start = track.start_time() as f64;
            let offset = (track.offset() as f64).clamp(0.0, *asset_duration);
            // A non-positive duration means "to the end of the asset", the
            // same thing the editor stores as null.
            let stored = track.duration() as f64;
            let length = if stored > 0.0 {
                stored
            } else {
                (asset_duration - offset).max(0.0)
            };
            if length <= 0.0 || time < start || time >= start + length {
                continue;
            }

            let into_clip = time - start;
            let source_position = if track.loop_() {
                let window = (asset_duration - offset).max(0.0);
                if window <= 0.0 {
                    continue;
                }
                offset + into_clip % window
            } else {
                let position = offset + into_clip;
                // Past the end of the file is silence, never a wrapped tail.
                if position >= *asset_duration {
                    continue;
                }
                position
            };

            audible.push((
                track.id().to_owned(),
                track.asset_id().to_owned(),
                (*file_name).to_owned(),
                source_position,
                (track.volume() as f64).clamp(0.0, 1.0),
            ));
        }
        Ok(audible)
    }

    /// Every audio file the document refers to, as
    /// `(asset_id, file_name, duration_seconds)`.
    ///
    /// The catalogue a host needs to *ask for* files: a `.kvk` names the audio
    /// it wants and carries none of it, so a reference app has to be able to
    /// say "this document needs calm-inspiring-technologie.mp3" before
    /// anything is bound (issue #111).
    ///
    /// Covers timeline tracks and machine sounds alike — both resolve through
    /// the same assets.
    pub fn audio_assets(&self) -> Result<Vec<(String, String, f64)>, format::InvalidFlatbuffer> {
        let document = self.document()?;
        Ok(document
            .audio_assets()
            .unwrap_or_default()
            .iter()
            .map(|asset| {
                (
                    asset.id().to_owned(),
                    asset.name().to_owned(),
                    asset.duration() as f64,
                )
            })
            .collect())
    }

    /// The document's state machines, as `(id, name)`.
    ///
    /// Introspection for a host that has to *show* a document rather than
    /// drive a known one — a reference app's machine picker (issue #111).
    pub fn state_machines(&self) -> Result<Vec<(String, String)>, format::InvalidFlatbuffer> {
        let document = self.document()?;
        Ok(document
            .state_machines()
            .unwrap_or_default()
            .iter()
            .map(|machine| (machine.id().to_owned(), machine.name().to_owned()))
            .collect())
    }

    /// One machine's inputs, as `(id, name, kind, default_bool, default_number)`.
    ///
    /// `kind` is the schema ordinal: 0 bool, 1 trigger, 2 number. Empty when
    /// no machine carries `machine_id`, which is the same answer as a machine
    /// with no inputs — a host renders both as "nothing to drive".
    pub fn machine_inputs(
        &self,
        machine_id: &str,
    ) -> Result<Vec<(String, String, u8, bool, f64)>, format::InvalidFlatbuffer> {
        let document = self.document()?;
        for machine in document.state_machines().unwrap_or_default() {
            if machine.id() != machine_id {
                continue;
            }
            return Ok(machine
                .inputs()
                .unwrap_or_default()
                .iter()
                .map(|input| {
                    (
                        input.id().to_owned(),
                        input.name().to_owned(),
                        input.kind().0 as u8,
                        input.default_bool_value(),
                        input.default_number_value() as f64,
                    )
                })
                .collect());
        }
        Ok(Vec::new())
    }

    /// The document's animation clips, as `(id, name, duration_seconds)`.
    pub fn animations(&self) -> Result<Vec<(String, String, f64)>, format::InvalidFlatbuffer> {
        let document = self.document()?;
        Ok(document
            .animations()
            .unwrap_or_default()
            .iter()
            .map(|clip| {
                (
                    clip.id().to_owned(),
                    clip.name().to_owned(),
                    clip.duration() as f64,
                )
            })
            .collect())
    }

    /// The document's audio events, as `(id, name, source_name)`.
    ///
    /// A `.kvk` carries no sound files, so this is how a host maps a reported
    /// firing onto whatever audio it ships.
    pub fn audio_events(
        &self,
    ) -> Result<Vec<(String, String, Option<String>)>, format::InvalidFlatbuffer> {
        let document = self.document()?;
        Ok(document
            .audio_events()
            .unwrap_or_default()
            .iter()
            .map(|event| {
                (
                    event.id().to_owned(),
                    event.name().to_owned(),
                    event.source_name().map(|name| name.to_owned()),
                )
            })
            .collect())
    }

    pub fn hit_test_document_point(
        &self,
        x: f64,
        y: f64,
    ) -> Result<Option<TapTarget>, format::InvalidFlatbuffer> {
        let document = self.document()?;
        // Hit-test the pose on screen, not the authored one: composed through
        // every group above the node, with this frame's overrides applied
        // (issue #74).
        let frame = self.evaluate_frame()?;
        let attached = crate::hierarchy::attach_worlds(&document, &frame.fk);
        let world =
            crate::hierarchy::world_affines_attached(&document, &frame.node_overrides, &attached);
        let nodes = document.nodes().unwrap_or_default();
        for node in nodes.iter().rev() {
            if !node.visible() {
                continue;
            }
            let affine = world.get(node.id()).copied().unwrap_or(IDENTITY);
            let Some(inverse) = invert(affine) else {
                continue;
            };
            let (local_x, local_y) = apply_point(inverse, x, y);
            if shape_contains(node, local_x, local_y) {
                return Ok(Some(TapTarget {
                    node_id: node.id().to_owned(),
                    node_name: node.name().to_owned(),
                }));
            }
        }
        Ok(None)
    }

    pub fn current_frame(&self) -> Result<EvaluatedFrame, format::InvalidFlatbuffer> {
        self.evaluate_frame()
    }

    pub fn document(&self) -> Result<format::Document<'_>, format::InvalidFlatbuffer> {
        format::load(&self.bytes)
    }

    fn advance_state_machines(&mut self, delta: f64) -> Result<(), format::InvalidFlatbuffer> {
        let document = self.document()?;
        let mut next = self.machines.clone();
        let mut fired_audio: Vec<String> = Vec::new();
        for machine in document.state_machines().unwrap_or_default() {
            let state = next
                .entry(machine.id().to_owned())
                .or_insert_with(MachineRuntimeState::new);
            // Every layer advances against the same input snapshot and the
            // same dt, in stack order, so a trigger one layer consumes is
            // still visible to the layers above it this frame. Consumption is
            // collected after the whole machine has advanced, never mid-loop.
            let snapshot = state.input_values.clone();
            let mut cleared = snapshot.clear_triggers();
            for graph in layers_of(machine) {
                if !graph.enabled() {
                    continue;
                }
                let result =
                    SmEvaluator::advance_layer(graph, &state.state_of(&graph), &snapshot, delta);
                state.set_state_of(&graph, result.state);
                fired_audio.extend(result.fired_audio_event_ids);
                // Any layer's non-trigger writes are already in `snapshot`;
                // only the trigger clearing needs merging back.
                cleared = result.input_values;
            }
            state.input_values = cleared;
        }
        self.machines = next;
        self.fired_audio_event_ids.extend(fired_audio);
        Ok(())
    }

    /// Folds every enabled layer above the base into `base_overrides`, in
    /// stack order, through the shared value-level seam (issue #96).
    ///
    /// Each layer contributes only the properties its own active clip
    /// animates, sampled at that layer's own elapsed seconds and wrapped into
    /// the clip's duration, so layers of different lengths stay in phase with
    /// their own loop. Later layers win where two of them animate the same
    /// property; everything else falls through to the layers below and, in the
    /// end, to the document's rest pose.
    fn apply_layer_poses(
        &self,
        document: &format::Document<'_>,
        base_overrides: HashMap<String, NodeOverrides>,
    ) -> HashMap<String, NodeOverrides> {
        let mut samples: Vec<HashMap<String, NodeOverrides>> = Vec::new();
        for machine in document.state_machines().unwrap_or_default() {
            let Some(runtime) = self.machines.get(machine.id()) else {
                continue;
            };
            let clocks: HashMap<String, LayerClock> = layers_of(machine)
                .iter()
                .filter(|graph| !graph.is_base())
                .filter_map(|graph| {
                    let state = runtime.state_of(graph);
                    state.active_state_id.clone().map(|active| {
                        (
                            graph.id(),
                            LayerClock {
                                active_state_id: active,
                                elapsed_seconds: state.source_state_elapsed_seconds,
                            },
                        )
                    })
                })
                .collect();
            samples.extend(layer_samples(document, machine, &clocks, &HashSet::new()));
        }
        fold_layer_samples(base_overrides, samples)
    }

    fn evaluate_frame(&self) -> Result<EvaluatedFrame, format::InvalidFlatbuffer> {
        let document = self.document()?;
        let active_animation = self.active_animation_at(document);
        // A transition crossfades, rather than cutting. `eased_progress` has
        // existed since the transition manager was written and its only caller
        // was a test: the runtime kept the *source* state active for the whole
        // duration and switched at the end, so every transition was a hard cut
        // on device while the editor faded (issue #111 parity audit §5.1).
        let mut node_overrides = match self.transition_pose(&document) {
            Some(pose) => pose,
            None => match self.blend_pose(&document) {
                Some(pose) => pose,
                None => active_animation
                    .map(|(animation, time)| evaluate_animation(&animation, time))
                    .unwrap_or_default(),
            },
        };
        // Layers above the base fold in on top, each sampled at its own clock
        // (issue #98). The base layer keeps taking `current_time` — it is the
        // clock `seek`, the loop mode and the host API all operate on — so a
        // document with no extra layers produces byte-for-byte the pose it
        // always did and never reaches the fold at all.
        node_overrides = self.apply_layer_poses(&document, node_overrides);
        // Merged over the clip's own overrides: a bound property beats a
        // keyframed one on the same property (issue #74).
        apply_bindings(&document, &self.number_values(), &mut node_overrides);

        let mut bones = bones_from_document(&document);
        for bone in &mut bones {
            let Some(overrides) = node_overrides.get(&bone.id) else {
                continue;
            };
            if let Some(rotation) = overrides.scalar("boneRotation") {
                bone.local_rotation = rotation;
            }
            // Stretch. This used to export as BoneRotation — the "closest
            // analogue" — which not only failed to stretch the bone but
            // *overwrote* any real rotation track on it, because overrides are
            // keyed by property (issue #111).
            if let Some(length) = overrides.scalar("boneLength") {
                bone.length = length.max(0.0);
            }
        }

        let mut targets = ik_targets_from_document(&document);
        for target in &mut targets {
            if let Some(overrides) = node_overrides.get(&target.id) {
                if let Some(x) = overrides.scalar("ikTargetPositionX") {
                    target.position.x = x;
                }
                if let Some(y) = overrides.scalar("ikTargetPositionY") {
                    target.position.y = y;
                }
            }
            // A following target is being pointed at the finger right now, and
            // that wins over a keyframe or a binding. The editor makes the
            // three interaction modes mutually exclusive, so a well-formed
            // document never has both driving one target — this only decides
            // the order for one that does.
            if let Some((x, y)) = self.follow_position.get(&target.id).copied() {
                target.position.x = x;
                target.position.y = y;
            }
        }

        // A group carries the targets inside it, the way it carries the
        // artwork inside it (issue #74). Applied after the keyframes, which
        // are authored in the same space as the stored position, and before
        // the solve, which reads world space.
        let carries = group_carry_deltas(&document, &node_overrides);
        if !carries.is_empty() {
            // A group carries the rig rooted in it, too: root bones only,
            // because FK carries their children off them (issue #74).
            for bone in &mut bones {
                let Some(group_id) = bone.group_id.as_deref() else {
                    continue;
                };
                if bone.parent_id.is_some() {
                    continue;
                }
                let Some(delta) = carries.get(group_id) else {
                    continue;
                };
                let (x, y) = apply_point(*delta, bone.local_position.x, bone.local_position.y);
                bone.local_position.x = x;
                bone.local_position.y = y;
                bone.local_rotation += delta[1].atan2(delta[0]);
            }
        }
        if !carries.is_empty() {
            for target in &mut targets {
                let Some(parent_id) = target.parent_id.as_deref() else {
                    continue;
                };
                let Some(delta) = carries.get(parent_id) else {
                    continue;
                };
                let (x, y) = apply_point(*delta, target.position.x, target.position.y);
                target.position.x = x;
                target.position.y = y;
                // A rotated group would shear an axis-aligned extent, which the
                // model cannot express, so only the scale is taken.
                let scale_x = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
                let scale_y = (delta[2] * delta[2] + delta[3] * delta[3]).sqrt();
                target.extent = (target.extent.0 * scale_x, target.extent.1 * scale_y);
            }
        }

        let chains = ik_chains_from_document(&document);
        let mut fk = if bones.is_empty() {
            HashMap::new()
        } else {
            evaluate_skeletal(&bones, &chains, &targets)
        };

        // Constraints run last, on the posed targets: an iris follows the
        // target *after* the timeline has moved it (issue #111). Their result
        // is written back as node overrides, which is how the rest of the
        // pipeline — renderer included — already reads a posed node.
        let target_positions: HashMap<String, (f64, f64)> = targets
            .iter()
            .map(|target| {
                (
                    target.id.clone(),
                    (target.position.x, target.position.y),
                )
            })
            .collect();
        // Bones first, right after FK and IK, as the editor's skeletal solve
        // does; then shapes, which may follow a bone.
        crate::bones::constraints::solve_bone_constraints(
            &document,
            &mut fk,
            &target_positions,
            &node_overrides,
        );
        let posed = crate::bones::constraints::solve_shape_constraints_with_bones(
            &document,
            &target_positions,
            &node_overrides,
            &fk,
        );
        let mut node_overrides = node_overrides;
        for (node_id, pose) in posed {
            let entry = node_overrides.entry(node_id).or_default();
            use crate::animation::PropertyValue;
            entry.values.insert("positionX", PropertyValue::Scalar(pose.x));
            entry.values.insert("positionY", PropertyValue::Scalar(pose.y));
            entry
                .values
                .insert("rotation", PropertyValue::Scalar(pose.rotation));
            entry
                .values
                .insert("scaleX", PropertyValue::Scalar(pose.scale_x));
            entry
                .values
                .insert("scaleY", PropertyValue::Scalar(pose.scale_y));
        }

        Ok(EvaluatedFrame {
            current_time: self.current_time,
            active_animation_id: active_animation
                .map(|(animation, _)| animation.id().to_owned()),
            node_overrides,
            fk,
            ik_targets: targets,
        })
    }

    /// Every machine's number inputs in one map, plus the declared defaults
    /// for inputs nobody has set. Bindings address inputs by id and do not
    /// care which machine declared one, and a document that ships a default
    /// should look the same on frame zero as it does after the host echoes
    /// that default back.
    fn number_values(&self) -> HashMap<String, f64> {
        let mut values = HashMap::new();
        if let Ok(document) = self.document() {
            for machine in document.state_machines().unwrap_or_default() {
                for input in machine.inputs().unwrap_or_default() {
                    if input.kind() == format::InputKind::Number {
                        values.insert(input.id().to_owned(), input.default_number_value() as f64);
                    }
                }
            }
        }
        for machine in self.machines.values() {
            for (input_id, value) in &machine.input_values.number_values {
                values.insert(input_id.clone(), value.0);
            }
        }
        values
    }

    fn active_animation(&self) -> Result<Option<format::Animation<'_>>, format::InvalidFlatbuffer> {
        let document = self.document()?;
        Ok(self.active_animation_from(document))
    }

    fn active_animation_timing(
        &self,
    ) -> Result<Option<(f64, format::LoopMode)>, format::InvalidFlatbuffer> {
        Ok(self
            .active_animation()?
            .map(|animation| (animation.duration() as f64, animation.loop_mode())))
    }

    /// The clip to pose from, and the time to sample it at.
    ///
    /// A machine-driven clip runs on its state's clock; everything else runs
    /// on the player's own playhead, which is what `seek` and the loop mode
    /// operate on.
    fn active_animation_at<'a>(
        &self,
        document: format::Document<'a>,
    ) -> Option<(format::Animation<'a>, f64)> {
        let animations = document.animations().unwrap_or_default();
        if let Some((id, elapsed)) = self.active_clip_from_state_machine(document) {
            if let Some(animation) = animations.iter().find(|animation| animation.id() == id) {
                let time = looped_time(&animation, elapsed);
                return Some((animation, time));
            }
        }
        self.active_animation_from(document)
            .map(|animation| (animation, self.current_time))
    }

    fn active_animation_from<'a>(
        &self,
        document: format::Document<'a>,
    ) -> Option<format::Animation<'a>> {
        let animations = document.animations().unwrap_or_default();
        if animations.is_empty() {
            return None;
        }

        if let Some(id) = self.active_clip_id_from_state_machine(document) {
            if let Some(animation) = animations.iter().find(|animation| animation.id() == id) {
                return Some(animation);
            }
        }

        if let Some(id) = self.active_animation_id.as_deref() {
            if let Some(animation) = animations.iter().find(|animation| animation.id() == id) {
                return Some(animation);
            }
        }

        Some(animations.get(0))
    }

    /// Delivers a pointer *move* — a drag, not a tap.
    ///
    /// Deliberately not a `ListenerEventKind`: the editor has no Move
    /// listener, and inventing one in the runtime's copy of that enum would
    /// put the two out of step. A move is continuous input, not a discrete
    /// event, so it drives the things that read the pointer continuously and
    /// fires no listeners at all.
    ///
    /// Without this a following target only updated on tap, so "look at my
    /// finger" meant "look where I last tapped" (issue #111).
    pub fn pointer_moved(&mut self, x: f64, y: f64) -> Result<(), format::InvalidFlatbuffer> {
        self.drive_pointer_inputs(x, y)?;
        self.drive_target_extent_bindings(x, y)?;
        self.aim_following_targets(x, y)
    }

    /// A point on the drawable, in document space.
    ///
    /// The host hit tests in the document's own coordinates, and the renderer
    /// now fits the document into the drawable and draws it through a camera.
    /// Undoing that on the host's side is how the two drift apart, so the
    /// conversion lives here, beside the transform it inverts.
    pub fn document_point(
        &self,
        surface_width: f64,
        surface_height: f64,
        x: f64,
        y: f64,
    ) -> Result<(f64, f64), format::InvalidFlatbuffer> {
        let document = self.document()?;
        let frame = self.evaluate_frame()?;
        let canvas = (
            document.canvas_width() as f64,
            document.canvas_height() as f64,
        );
        Ok(crate::render::document_point(
            canvas,
            crate::render::camera_affine(&document, &frame, canvas),
            (surface_width, surface_height),
            x,
            y,
        ))
    }

    /// The artboard background the surface clears to, as straight RGBA
    /// (issue #233). `[0, 0, 0, 0]` is transparent — no background, or a
    /// fully clear one — so a host reads one shape either way.
    pub fn background_color(&self) -> Result<[u8; 4], format::InvalidFlatbuffer> {
        let document = self.document()?;
        Ok(crate::render::vello_scene::background_of(&document).unwrap_or([0; 4]))
    }

    /// Points every pointer-following target at the finger.
    ///
    /// The third interaction mode, "exposes": the target's position *becomes*
    /// the pointer's, so a constraint reading it aims whatever it drives — an
    /// iris, a head — at the touch. None of it travelled before issue #111, so
    /// a document configured this way did nothing at all at runtime while the
    /// editor's preview followed along.
    ///
    /// Mirrors `_updateFollowTargets` (`preview_pointer_drive.dart:188-201`):
    /// scope `Anywhere` always qualifies, `WithinExtent` only while the finger
    /// is inside the target's own box — and a target with no extent has no box
    /// to be inside, so it never qualifies.
    fn aim_following_targets(&mut self, x: f64, y: f64) -> Result<(), format::InvalidFlatbuffer> {
        let document = self.document()?;
        let mut aimed: Vec<String> = Vec::new();
        for target in document.ik_targets().unwrap_or_default().iter() {
            if !target.follows_pointer() {
                continue;
            }
            if target.follow_scope() == format::generated::kineto::TargetFollowScope::WithinExtent {
                let (half_w, half_h) = (
                    target.extent_w() as f64 / 2.0,
                    target.extent_h() as f64 / 2.0,
                );
                if half_w <= 0.0 || half_h <= 0.0 {
                    continue;
                }
                let (cx, cy) = (target.position_x() as f64, target.position_y() as f64);
                if (x - cx).abs() > half_w || (y - cy).abs() > half_h {
                    continue;
                }
            }
            aimed.push(target.id().to_owned());
        }
        for id in aimed {
            self.follow_desired.insert(id, (x, y));
        }
        Ok(())
    }

    /// Moves each following target toward where the pointer last asked it to
    /// be, in the feel its author chose.
    ///
    /// The constants are the editor's (`preview_pointer_drive.dart:166-180`)
    /// rather than ones that merely look similar: a chase that settles at a
    /// different rate reads as a different animation.
    fn chase_following_targets(&mut self, dt: f64) {
        if self.follow_desired.is_empty() || dt <= 0.0 {
            return;
        }
        let feels: HashMap<String, (u8, (f64, f64))> = match self.document() {
            Ok(document) => document
                .ik_targets()
                .unwrap_or_default()
                .iter()
                .map(|target| {
                    (
                        target.id().to_owned(),
                        (
                            target.follow_feel().0,
                            (target.position_x() as f64, target.position_y() as f64),
                        ),
                    )
                })
                .collect(),
            Err(_) => return,
        };

        for (id, desired) in self.follow_desired.clone() {
            let Some((feel, authored)) = feels.get(&id).copied() else {
                continue;
            };
            let have = self.follow_position.get(&id).copied().unwrap_or(authored);
            let next = match feel {
                // Smooth: an exponential ease with the editor's rate.
                1 => {
                    let t = (dt * 10.0).clamp(0.0, 1.0);
                    (
                        have.0 + (desired.0 - have.0) * t,
                        have.1 + (desired.1 - have.1) * t,
                    )
                }
                // Spring: overshoots, then settles.
                2 => {
                    const STIFFNESS: f64 = 220.0;
                    const DAMPING: f64 = 18.0;
                    let velocity = self.follow_velocity.get(&id).copied().unwrap_or((0.0, 0.0));
                    let accel = (
                        (desired.0 - have.0) * STIFFNESS - velocity.0 * DAMPING,
                        (desired.1 - have.1) * STIFFNESS - velocity.1 * DAMPING,
                    );
                    let next_velocity = (velocity.0 + accel.0 * dt, velocity.1 + accel.1 * dt);
                    self.follow_velocity.insert(id.clone(), next_velocity);
                    (
                        have.0 + next_velocity.0 * dt,
                        have.1 + next_velocity.1 * dt,
                    )
                }
                // Snap, and any feel a newer file carries that this build does
                // not know: land on the pointer.
                _ => desired,
            };
            self.follow_position.insert(id, next);
        }
    }

    /// Writes the pointer's position within an interactive target's extent to
    /// the inputs bound to that target's axes.
    ///
    /// This is the other half of a machinery that was only ever half built.
    /// `apply_bindings` already maps a 0..1 input across a target's extent, so
    /// the *reading* side worked — but nothing wrote the number, so a target
    /// the author made interactive did nothing at runtime while the editor's
    /// preview moved it. `hit_test_targets` existed and was used only to pick
    /// a subject for listeners, which is why a tap logged "0 listeners" on a
    /// document whose targets were never driven by listeners in the first
    /// place (issue #111).
    ///
    /// Mirrors `_driveWithinExtent` (`preview_pointer_drive.dart:419-452`),
    /// including its deliberate indifference to the input's `source`: an input
    /// dragged this way is doing what a pointer input would, and the source
    /// only says who writes it once the document ships.
    fn drive_target_extent_bindings(
        &mut self,
        x: f64,
        y: f64,
    ) -> Result<(), format::InvalidFlatbuffer> {
        let document = self.document()?;
        let bindings = document.input_bindings().unwrap_or_default();
        if bindings.is_empty() {
            return Ok(());
        }

        // The *authored* targets, deliberately, not the evaluated ones.
        //
        // `apply_bindings` anchors a binding's travel to the position stored
        // in the document (`bindings.rs:68-77`), so the box a drag maps into
        // has to be the same one. Reading the evaluated position instead makes
        // the box chase the thing being dragged: with the input at its default
        // the target already sits at the extent's lower edge, the box recentres
        // there, and a tap at the other end lands outside it and does nothing.
        let mut writes: Vec<(String, f64)> = Vec::new();
        for target in document.ik_targets().unwrap_or_default().iter() {
            if !target.interactive() {
                continue;
            }
            let (half_w, half_h) = (
                target.extent_w() as f64 / 2.0,
                target.extent_h() as f64 / 2.0,
            );
            if half_w <= 0.0 || half_h <= 0.0 {
                continue;
            }
            let (cx, cy) = (target.position_x() as f64, target.position_y() as f64);
            // Only a pointer inside the travel box drives it.
            if (x - cx).abs() > half_w || (y - cy).abs() > half_h {
                continue;
            }

            for binding in bindings.iter() {
                if binding.subject_id() != target.id() {
                    continue;
                }
                let (value, min, max) = match binding.property() {
                    format::AnimatedPropertyKind::IkTargetPositionX => {
                        (x, cx - half_w, cx + half_w)
                    }
                    format::AnimatedPropertyKind::IkTargetPositionY => {
                        (y, cy - half_h, cy + half_h)
                    }
                    // A binding on this target that drives something else is
                    // not a spatial drag.
                    _ => continue,
                };
                if max == min {
                    continue;
                }
                let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
                let written = if binding.invert() { 1.0 - t } else { t };
                writes.push((binding.input_id().to_owned(), written));
            }
        }

        self.write_number_inputs(writes);
        Ok(())
    }

    /// Writes raw values to number inputs by id, finding the machine each
    /// belongs to.
    fn write_number_inputs(&mut self, writes: Vec<(String, f64)>) {
        if writes.is_empty() {
            return;
        }
        let owners: Vec<(String, String)> = match self.document() {
            Ok(document) => document
                .state_machines()
                .unwrap_or_default()
                .iter()
                .flat_map(|machine| {
                    let machine_id = machine.id().to_owned();
                    machine
                        .inputs()
                        .unwrap_or_default()
                        .iter()
                        .map(|input| (input.id().to_owned(), machine_id.clone()))
                        .collect::<Vec<_>>()
                })
                .collect(),
            Err(_) => return,
        };

        for (input_id, value) in writes {
            let Some((_, machine_id)) = owners.iter().find(|(id, _)| *id == input_id) else {
                continue;
            };
            if let Some(runtime) = self.machines.get_mut(machine_id) {
                runtime
                    .input_values
                    .number_values
                    .insert(input_id, crate::state_machine::OrderedF64(value));
            }
        }
    }

    /// Feeds the pointer's normalised position to every input that asks for it.
    ///
    /// Normalised against the document's canvas and clamped, so a drag past
    /// the edge holds at 0 or 1 rather than running away — the same box the
    /// editor measures against.
    fn drive_pointer_inputs(&mut self, x: f64, y: f64) -> Result<(), format::InvalidFlatbuffer> {
        let document = self.document()?;
        let width = document.canvas_width() as f64;
        let height = document.canvas_height() as f64;
        if width <= 0.0 || height <= 0.0 {
            return Ok(());
        }
        let nx = (x / width).clamp(0.0, 1.0);
        let ny = (y / height).clamp(0.0, 1.0);

        let mut writes: Vec<(String, String, f64)> = Vec::new();
        for machine in document.state_machines().unwrap_or_default() {
            for input in machine.inputs().unwrap_or_default() {
                if input.kind() != format::InputKind::Number {
                    continue;
                }
                let value = match input.source() {
                    format::generated::kineto::InputSource::PointerX => nx,
                    format::generated::kineto::InputSource::PointerY => ny,
                    // Manual, and any source a newer file carries that this
                    // build does not know: left alone rather than guessed at.
                    _ => continue,
                };
                writes.push((machine.id().to_owned(), input.id().to_owned(), value));
            }
        }

        for (machine_id, input_id, value) in writes {
            if let Some(runtime) = self.machines.get_mut(&machine_id) {
                runtime
                    .input_values
                    .number_values
                    .insert(input_id, crate::state_machine::OrderedF64(value));
            }
        }
        Ok(())
    }

    /// The pose of an active blend state, or `None` when the active state is a
    /// plain animation.
    ///
    /// A blend state used to export as an animation state with an empty clip
    /// id, so it posed nothing at all on device while the editor blended
    /// (issue #111). The weights already partition 1 and carry no priority,
    /// which is what `mix_samples` means — so a 50/50 blend is the average of
    /// two poses rather than one of them winning.
    fn blend_pose(
        &self,
        document: &format::Document<'_>,
    ) -> Option<HashMap<String, NodeOverrides>> {
        let animations = document.animations().unwrap_or_default();
        let numbers = self.number_values();
        let lookup = |id: &str| numbers.get(id).copied();

        for machine in document.state_machines().unwrap_or_default() {
            let Some(runtime) = self.machines.get(machine.id()) else {
                continue;
            };
            let Some(active) = runtime.eval_state.active_state_id.as_deref() else {
                continue;
            };
            let Some(state) = machine
                .states()
                .unwrap_or_default()
                .iter()
                .find(|s| s.id() == active)
            else {
                continue;
            };
            if state.kind() == format::generated::kineto::SMStateKind::Animation {
                continue;
            }

            let weights = weights_for_state(state, &lookup);
            let elapsed = runtime.eval_state.source_state_elapsed_seconds;
            let mut samples = Vec::new();
            for entry in weights {
                if entry.weight <= 0.0 {
                    continue;
                }
                // A dangling clip id contributes nothing rather than
                // collapsing the whole blend.
                let Some(animation) = animations.iter().find(|a| a.id() == entry.clip_id) else {
                    continue;
                };
                samples.push(
                    LayerSample::new(into_value_maps(evaluate_animation(
                        &animation,
                        looped_time(&animation, elapsed),
                    )))
                    .with_weight(entry.weight),
                );
            }
            if samples.is_empty() {
                continue;
            }
            return Some(from_value_maps(mix_samples(&samples)));
        }

        None
    }

    /// The blended pose while a transition is running, or `None` when none is.
    ///
    /// Ordered as the editor's preview builds it
    /// (`logic_preview_panel.dart:1216-1256`): the source clip is the floor at
    /// full weight and the destination fades in over it, so the pair resolves
    /// to `lerp(from, to, progress)` — a real value interpolation. Two
    /// independent coverages would ghost instead.
    fn transition_pose(
        &self,
        document: &format::Document<'_>,
    ) -> Option<HashMap<String, NodeOverrides>> {
        let animations = document.animations().unwrap_or_default();

        for machine in document.state_machines().unwrap_or_default() {
            let Some(runtime) = self.machines.get(machine.id()) else {
                continue;
            };
            let state = &runtime.eval_state;
            let Some(transition_id) = state.active_transition_id.as_deref() else {
                continue;
            };
            let Some(transition) = machine
                .transitions()
                .unwrap_or_default()
                .iter()
                .find(|t| t.id() == transition_id)
            else {
                continue;
            };

            let clip_of = |state_id: &str| -> Option<format::Animation<'_>> {
                let clip_id = machine
                    .states()
                    .unwrap_or_default()
                    .iter()
                    .find(|s| s.id() == state_id)
                    .map(|s| s.clip_id().to_owned())?;
                animations.iter().find(|a| a.id() == clip_id)
            };

            let from = clip_of(transition.from_state_id());
            let to = clip_of(transition.to_state_id());
            if from.is_none() && to.is_none() {
                continue;
            }

            let progress = TransitionManager::eased_progress(state, transition);
            let mut samples = Vec::new();
            if let Some(animation) = &from {
                samples.push(LayerSample::new(into_value_maps(evaluate_animation(
                    animation,
                    looped_time(animation, state.source_state_elapsed_seconds),
                ))));
            }
            if let Some(animation) = &to {
                // Full weight when there is no source underneath: fading in
                // from nothing would leave the first frames posed by the rest
                // pose instead of the clip.
                let weight = if from.is_none() { 1.0 } else { progress };
                samples.push(
                    LayerSample::new(into_value_maps(evaluate_animation(
                        animation,
                        looped_time(animation, state.destination_state_elapsed_seconds),
                    )))
                    .with_weight(weight),
                );
            }

            return Some(from_value_maps(blend_ordered(&samples, None)));
        }

        None
    }

    fn active_clip_id_from_state_machine(&self, document: format::Document<'_>) -> Option<String> {
        self.active_clip_from_state_machine(document)
            .map(|(clip_id, _)| clip_id)
    }

    /// The clip a machine is holding, and *how long it has been holding it*.
    ///
    /// The elapsed time is the point. A machine's clip runs on that state's own
    /// clock, which restarts when the state is entered — that is what the
    /// editor samples (`logic_preview_panel.dart:1266`). The runtime sampled
    /// the player's single global playhead instead, and the two diverge the
    /// moment a machine changes state: a one-shot clip finishing sets that
    /// playhead to its duration and stops the player, so coming back to a
    /// looping clip rendered its last frame forever (issue #111).
    fn active_clip_from_state_machine(
        &self,
        document: format::Document<'_>,
    ) -> Option<(String, f64)> {
        for machine in document.state_machines().unwrap_or_default() {
            let Some(runtime) = self.machines.get(machine.id()) else {
                continue;
            };
            let active_state = runtime.eval_state.active_state_id.as_deref()?;
            if let Some(state) = machine
                .states()
                .unwrap_or_default()
                .iter()
                .find(|state| state.id() == active_state)
            {
                return Some((
                    state.clip_id().to_owned(),
                    runtime.eval_state.source_state_elapsed_seconds,
                ));
            }
        }
        None
    }

    /// Advances `current_time` for `PingPong` loop mode using explicit
    /// direction state (see the `direction` field doc comment). The `while`
    /// loop handles a single oversized `delta` bouncing across more than one
    /// reflection, landing on the correct phase instead of overshooting.
    fn advance_ping_pong(&mut self, delta: f64, duration: f64) {
        if duration <= 0.0 {
            self.current_time = 0.0;
            return;
        }
        let mut t = self.current_time + delta * self.direction as f64;
        let mut dir = self.direction;
        while t > duration || t < 0.0 {
            if t > duration {
                t = 2.0 * duration - t;
                dir = -dir;
            }
            if t < 0.0 {
                t = -t;
                dir = -dir;
            }
        }
        self.current_time = t;
        self.direction = dir;
    }

    /// Handles `Loop` and `OneShot` wrapping. `PingPong` is advanced via
    /// [`Self::advance_ping_pong`] instead — the bounced-time-only approach
    /// this used to also handle for `PingPong` lost cycle parity and froze
    /// playback at the clip end (issue #43).
    fn clamp_or_loop(&mut self, duration: f64, loop_mode: format::LoopMode) {
        if duration <= 0.0 || self.current_time < duration {
            return;
        }

        match loop_mode {
            format::LoopMode::Loop => self.current_time %= duration,
            format::LoopMode::PingPong => unreachable!("PingPong uses advance_ping_pong"),
            _ => {
                self.current_time = duration;
                self.play_state = PlayState::Stopped;
            }
        }
    }

    fn find_input_id(
        &self,
        state_machine_id: &str,
        input_name: &str,
        kind: format::InputKind,
    ) -> Result<String, String> {
        let document = self.document().map_err(|error| error.to_string())?;
        let Some(machine) = document
            .state_machines()
            .unwrap_or_default()
            .iter()
            .find(|machine| machine.id() == state_machine_id)
        else {
            return Err(format!("Unknown state machine id \"{state_machine_id}\""));
        };
        let input = machine
            .inputs()
            .unwrap_or_default()
            .iter()
            .find(|input| input.name() == input_name && input.kind() == kind);
        input
            .map(|input| input.id().to_owned())
            .ok_or_else(|| format!("Unknown input name \"{input_name}\""))
    }
}

/// Whether the shape covers a point **in its own local space** — the caller
/// brings the point there through the node's composed world transform, so
/// rotation, anchors and every group above it are already accounted for.
/// The input a listener writes to, looked up on its own machine.
///
/// Only Toggle needs this: Set carries its value in the listener, but a flip
/// has to start from somewhere, and an input that has never been written
/// starts at the value the document authored.
fn find_machine_input<'a>(
    machine: format::StateMachine<'a>,
    id: &str,
) -> Option<format::StateMachineInput<'a>> {
    machine
        .inputs()
        .unwrap_or_default()
        .iter()
        .find(|input| input.id() == id)
}

fn shape_contains(node: format::Node<'_>, x: f64, y: f64) -> bool {
    let Some(shape) = node.component_as_shape() else {
        return false;
    };

    if let Some(rect) = shape.geometry_as_rectangle() {
        let width = rect.width() as f64;
        let height = rect.height() as f64;
        return x >= 0.0 && x <= width && y >= 0.0 && y <= height;
    }

    if let Some(ellipse) = shape.geometry_as_ellipse() {
        let width = ellipse.width() as f64;
        let height = ellipse.height() as f64;
        if width.abs() < f64::EPSILON || height.abs() < f64::EPSILON {
            return false;
        }
        let nx = (x - width / 2.0) / (width / 2.0);
        let ny = (y - height / 2.0) / (height / 2.0);
        return nx * nx + ny * ny <= 1.0;
    }

    false
}

/// Wraps `elapsed` into `animation`'s own duration, honouring its loop mode,
/// so blended layers of different lengths each stay in phase with their own
/// loop rather than freezing at the end of the shortest one (issue #98).
fn looped_time(animation: &format::Animation<'_>, elapsed: f64) -> f64 {
    let duration = animation.duration() as f64;
    if duration <= 0.0 {
        return 0.0;
    }
    match animation.loop_mode() {
        format::LoopMode::PingPong => {
            let period = duration * 2.0;
            let t = elapsed % period;
            if t <= duration {
                t
            } else {
                period - t
            }
        }
        format::LoopMode::OneShot if !animation.loop_() => elapsed.clamp(0.0, duration),
        _ => elapsed % duration,
    }
}

/// `NodeOverrides` → the owned-key shape the blend seam works on.
fn into_value_maps(
    overrides: HashMap<String, NodeOverrides>,
) -> HashMap<String, HashMap<String, PropertyValue>> {
    overrides
        .into_iter()
        .map(|(node_id, values)| {
            (
                node_id,
                values
                    .values
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect(),
            )
        })
        .collect()
}

/// The inverse. Keys the runtime does not know are dropped rather than
/// guessed at — `NodeOverrides` stores `&'static str`, and every key in play
/// came from `property_key` in the first place.
fn from_value_maps(
    maps: HashMap<String, HashMap<String, PropertyValue>>,
) -> HashMap<String, NodeOverrides> {
    maps.into_iter()
        .map(|(node_id, values)| {
            let mut overrides = NodeOverrides::default();
            for (key, value) in values {
                if let Some(key) = static_property_key(&key) {
                    overrides.values.insert(key, value);
                }
            }
            (node_id, overrides)
        })
        .collect()
}

/// Where one layer's own clock has got to (issue #98) — the Rust mirror of
/// Dart's `LayerClock`.
#[derive(Debug, Clone)]
pub struct LayerClock {
    pub active_state_id: String,
    pub elapsed_seconds: f64,
}

/// The pose each non-base layer of `machine` contributes, in stack order.
///
/// The shared half of the runtime parity contract: Dart's
/// `evaluateLayeredPose` follows the same rules, and both are checked against
/// `test/fixtures/layers/body_face_mouth_poses.txt`.
///
///  * a layer that is disabled — by its own flag or by `disabled` — is skipped
///    entirely and contributes nothing;
///  * a layer with no clock sits in its own initial state at time zero, the
///    pose before anything has advanced;
///  * each layer samples its clip at its *own* elapsed seconds, wrapped into
///    that clip's duration and loop mode.
pub fn layer_samples(
    document: &format::Document<'_>,
    machine: format::StateMachine<'_>,
    clocks: &HashMap<String, LayerClock>,
    disabled: &HashSet<String>,
) -> Vec<HashMap<String, NodeOverrides>> {
    let animations = document.animations().unwrap_or_default();
    let mut samples = Vec::new();

    for graph in layers_of(machine) {
        if graph.is_base() {
            continue;
        }
        let id = graph.id();
        if !graph.enabled() || disabled.contains(&id) {
            continue;
        }
        let state = match clocks.get(&id) {
            Some(clock) => SmEvalState {
                active_state_id: Some(clock.active_state_id.clone()),
                source_state_elapsed_seconds: clock.elapsed_seconds,
                ..Default::default()
            },
            None => SmEvalState {
                active_state_id: graph.initial_state_id_resolved(),
                ..Default::default()
            },
        };
        let Some((clip_id, elapsed)) = graph.active_clip(&state) else {
            continue;
        };
        let Some(animation) = animations.iter().find(|a| a.id() == clip_id) else {
            continue;
        };
        let overrides = evaluate_animation(&animation, looped_time(&animation, elapsed));
        if overrides.is_empty() {
            continue;
        }
        samples.push(overrides);
    }
    samples
}

/// Folds `samples` over `base` through the shared value-level seam.
///
/// With nothing stacked on it the base pose is returned untouched, so a
/// document with no extra layers never reaches the fold at all.
pub fn fold_layer_samples(
    base: HashMap<String, NodeOverrides>,
    samples: Vec<HashMap<String, NodeOverrides>>,
) -> HashMap<String, NodeOverrides> {
    if samples.is_empty() {
        return base;
    }
    let mut ordered = Vec::with_capacity(samples.len() + 1);
    ordered.push(LayerSample::new(into_value_maps(base)));
    for sample in samples {
        ordered.push(LayerSample::new(into_value_maps(sample)));
    }
    from_value_maps(blend_ordered(&ordered, None))
}
