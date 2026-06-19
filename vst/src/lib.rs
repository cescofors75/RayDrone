// RayDrone (simplified) — a VST3 / CLAP instrument.
//
// Loads a WAV "scene" and renders an evolving drone by casting stochastic grains
// ("rays") around a focal point, exactly as the RayDrone methodology describes:
// the drone is not stored in the sample — it emerges from the convergence of N rays.
//
// Built with nih-plug (Rust). The DSP lives in `engine.rs`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};

mod engine;
use engine::{Engine, VLOG};

/// Source samples + native sample-rate waiting to be picked up by the audio thread.
/// A scene-change command handed from the GUI thread to the audio thread.
enum SceneCmd {
    /// Replace the scene with decoded audio (samples, sample_rate).
    Wav(Vec<f32>, f32),
    /// Switch to live-input capture of the last N seconds of host audio.
    Live(f32),
}
type PendingSample = Arc<Mutex<Option<SceneCmd>>>;

/// Snapshot of the engine state for the visualizer. The audio thread writes it
/// (try-lock, once per block); the GUI thread reads it each frame.
#[derive(Clone)]
struct VizState {
    level: f32,
    focus: f32,
    aperture: f32,
    rays: Vec<f32>, // ring buffer of normalized ray positions, length VLOG
    ray_w: usize,
    wave: Vec<f32>, // downsampled |peaks| of the scene, 0..1
}

impl Default for VizState {
    fn default() -> Self {
        Self {
            level: 0.0,
            focus: 0.3,
            aperture: 0.1,
            rays: vec![0.5; VLOG],
            ray_w: 0,
            wave: Vec::new(),
        }
    }
}

type Viz = Arc<Mutex<VizState>>;

pub struct RayDrone {
    params: Arc<RayDroneParams>,
    engine: Engine,
    /// A freshly-loaded WAV handed from the GUI thread to the audio thread.
    pending: PendingSample,
    /// Human-readable name of the loaded sample (for the GUI).
    sample_name: Arc<Mutex<String>>,
    /// Live state for the ray visualizer.
    viz: Viz,
    /// Notes currently held via incoming MIDI (persists across blocks).
    midi_held: [bool; 128],
    /// Notes currently pressed on the on-screen piano (shared with the editor).
    keys: Arc<Mutex<[bool; 128]>>,
}

#[derive(Params)]
struct RayDroneParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    /// Absolute path of the loaded WAV, persisted so DAW projects recall the scene.
    #[persist = "sample-path"]
    sample_path: Arc<Mutex<Option<String>>>,

    /// Density of the render — grains ("rays") per second. More → smoother drone.
    #[id = "density"]
    density: FloatParam,
    /// Aperture / depth of field — width of the temporal dispersion cone (ms).
    #[id = "aperture"]
    aperture: FloatParam,
    /// Focal point — where in the sample the rays are cast around (0..1).
    #[id = "focus"]
    focus: FloatParam,
    /// Reverb wet mix.
    #[id = "reverb"]
    reverb: FloatParam,
    /// Autoevolution — recursive feedback. The output drifts focus & aperture on its own.
    #[id = "evolve"]
    evolve: FloatParam,
    /// Shimmer — probability a grain plays an octave up (airy sheen).
    #[id = "shimmer"]
    shimmer: FloatParam,
    /// Bounce — recursive ray depth: how many times a grain relaunches a child.
    #[id = "bounce"]
    bounce: FloatParam,
    /// Reflect — probability each bounce survives (tail energy & length).
    #[id = "reflect"]
    reflect: FloatParam,
    /// Keys — how much held notes take over vs. the base drone (0 = drone only).
    #[id = "keymix"]
    keymix: FloatParam,
    /// Dry/Wet — blend of the original signal and the rendered drone.
    #[id = "mix"]
    mix: FloatParam,
    /// Master output level.
    #[id = "master"]
    master: FloatParam,
    /// Bypass — pass the input through untouched.
    #[id = "bypass"]
    bypass: BoolParam,
}

impl Default for RayDrone {
    fn default() -> Self {
        Self {
            params: Arc::new(RayDroneParams::default()),
            engine: Engine::new(44100.0),
            pending: Arc::new(Mutex::new(None)),
            sample_name: Arc::new(Mutex::new("(no sample)".to_string())),
            viz: Arc::new(Mutex::new(VizState::default())),
            midi_held: [false; 128],
            keys: Arc::new(Mutex::new([false; 128])),
        }
    }
}

impl Default for RayDroneParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(760, 580),
            sample_path: Arc::new(Mutex::new(None)),

            density: FloatParam::new(
                "Density",
                200.0,
                FloatRange::Skewed { min: 10.0, max: 500.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" rays/s")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            aperture: FloatParam::new(
                "Aperture",
                100.0,
                FloatRange::Skewed { min: 1.0, max: 2000.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            focus: FloatParam::new("Focus", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            reverb: FloatParam::new("Reverb", 0.2, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            evolve: FloatParam::new("Evolve", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            shimmer: FloatParam::new("Shimmer", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            bounce: FloatParam::new("Bounce", 0.0, FloatRange::Linear { min: 0.0, max: 6.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(0)),

            reflect: FloatParam::new("Reflect", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            keymix: FloatParam::new("Keys", 0.6, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            mix: FloatParam::new("Mix", 0.7, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage())
                .with_smoother(SmoothingStyle::Linear(20.0)),

            bypass: BoolParam::new("Bypass", false).make_bypass(),

            master: FloatParam::new(
                "Master",
                util::db_to_gain(-6.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(0.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 0.0),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db())
            .with_smoother(SmoothingStyle::Logarithmic(50.0)),
        }
    }
}

impl Plugin for RayDrone {
    const NAME: &'static str = "RayDrone";
    const VENDOR: &'static str = "RayDrone";
    const URL: &'static str = "https://github.com/cescofors75/raydrone";
    const EMAIL: &'static str = "noreply@example.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Effect: stereo in (the "scene" / dry signal), stereo out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    // Accept MIDI notes so you can play the drone from a keyboard.
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let pending = self.pending.clone();
        let sample_name = self.sample_name.clone();
        let viz = self.viz.clone();
        let keys = self.keys.clone();
        let egui_state = self.params.editor_state.clone();

        create_egui_editor(
            egui_state,
            (),
            |_, _| {},
            move |ctx, setter, _state| {
                draw_ui(ctx, setter, &params, &pending, &sample_name, &viz, &keys);
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.engine.set_sample_rate(buffer_config.sample_rate);

        // Restore a persisted scene (DAW project recall), or fall back to a
        // built-in so the plugin makes sound out of the box.
        {
            let path = self.params.sample_path.lock().unwrap().clone();
            // Live-input mode was saved with the project.
            if matches!(path.as_deref(), Some("live")) {
                self.engine.begin_live_capture(6.0);
                if let Ok(mut v) = self.viz.lock() {
                    v.wave = Vec::new();
                }
                *self.sample_name.lock().unwrap() = "live input".to_string();
                return true;
            }
        }
        if self.engine.is_empty() {
            let path = self.params.sample_path.lock().unwrap().clone();
            let (data, sr, name): (Vec<f32>, f32, String) = match path {
                // A built-in scene was saved with the project.
                Some(p) if p.starts_with("builtin:") => {
                    let scene = scene_from_id(&p["builtin:".len()..]).unwrap_or(Scene::Pad);
                    let (d, s) = builtin_scene(scene);
                    (d, s, format!("built-in: {}", scene_id(scene)))
                }
                // A WAV file path was saved.
                Some(p) => match load_wav(Path::new(&p)) {
                    Ok((d, s)) => (d, s, file_label(&p)),
                    // File missing/moved → don't leave it silent.
                    Err(_) => {
                        let (d, s) = builtin_scene(Scene::Pad);
                        (d, s, "built-in: Pad".to_string())
                    }
                },
                // Fresh instance → default built-in Pad.
                None => {
                    let (d, s) = builtin_scene(Scene::Pad);
                    *self.params.sample_path.lock().unwrap() = Some("builtin:Pad".to_string());
                    (d, s, "built-in: Pad".to_string())
                }
            };
            let peaks = wave_peaks(&data, 256);
            self.engine.load(data, sr);
            if let Ok(mut v) = self.viz.lock() {
                v.wave = peaks;
            }
            *self.sample_name.lock().unwrap() = name;
        }
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Pick up a scene change from the GUI thread (rare event).
        if let Ok(mut slot) = self.pending.try_lock() {
            match slot.take() {
                Some(SceneCmd::Wav(data, sr)) => self.engine.load(data, sr),
                Some(SceneCmd::Live(secs)) => self.engine.begin_live_capture(secs),
                None => {}
            }
        }

        // Collect MIDI note on/off (block-accurate is fine for a drone).
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, .. } => {
                    if let Some(s) = self.midi_held.get_mut(note as usize) {
                        *s = true;
                    }
                }
                NoteEvent::NoteOff { note, .. } | NoteEvent::Choke { note, .. } => {
                    if let Some(s) = self.midi_held.get_mut(note as usize) {
                        *s = false;
                    }
                }
                _ => {}
            }
        }
        // Union of MIDI notes and the on-screen piano → held pitches.
        let mut held = self.midi_held;
        if let Ok(ui_keys) = self.keys.try_lock() {
            for (h, &k) in held.iter_mut().zip(ui_keys.iter()) {
                *h |= k;
            }
        }
        self.engine.set_keys(&held, 60);

        // Bypass: pass the input straight through, untouched.
        if self.params.bypass.value() {
            return ProcessStatus::Normal;
        }

        // Push current parameter values to the engine.
        self.engine.set_density(self.params.density.value());
        self.engine.set_aperture_ms(self.params.aperture.value());
        self.engine.set_focus(self.params.focus.value());
        self.engine.set_reverb(self.params.reverb.value());
        self.engine.set_feedback(self.params.evolve.value());
        self.engine.set_octave(self.params.shimmer.value());
        self.engine.set_bounce(self.params.bounce.value().round() as u32);
        self.engine.set_reflect(self.params.reflect.value());
        self.engine.set_key_mix(self.params.keymix.value());

        for mut frame in buffer.iter_samples() {
            self.engine.set_master(self.params.master.smoothed.next());
            let mix = self.params.mix.smoothed.next();

            // Read the dry input (the "scene") before overwriting it.
            let mut ch = frame.iter_mut();
            let c0 = ch.next();
            let c1 = ch.next();
            let il = c0.as_deref().copied().unwrap_or(0.0);
            let ir = c1.as_deref().copied().unwrap_or(il);

            // Feed the input into the live-capture buffer, then render one frame.
            self.engine.push_input(0.5 * (il + ir));
            let (dl, dr) = self.engine.tick();

            // Dry/Wet blend: the rays modify the original into an ambient texture.
            let ol = (1.0 - mix) * il + mix * dl;
            let or_ = (1.0 - mix) * ir + mix * dr;
            if let Some(s) = c0 {
                *s = ol;
            }
            if let Some(s) = c1 {
                *s = or_;
            }
        }

        // Hand a fresh snapshot to the visualizer (rare contention with the GUI).
        if let Ok(mut v) = self.viz.try_lock() {
            v.level = self.engine.viz_level();
            v.focus = self.engine.viz_focus();
            v.aperture = self.engine.viz_aperture();
            v.ray_w = self.engine.ray_write();
            v.rays.copy_from_slice(self.engine.ray_buffer());
        }

        ProcessStatus::Normal
    }
}

// Palette (2026 neon-on-near-black).
const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 53, 94); // #ff355e
const CYAN: egui::Color32 = egui::Color32::from_rgb(40, 230, 220);
const BG0: egui::Color32 = egui::Color32::from_rgb(9, 9, 14);
const BG1: egui::Color32 = egui::Color32::from_rgb(16, 16, 24);

fn draw_ui(
    ctx: &egui::Context,
    setter: &ParamSetter,
    params: &Arc<RayDroneParams>,
    pending: &PendingSample,
    sample_name: &Arc<Mutex<String>>,
    viz: &Viz,
    keys: &Arc<Mutex<[bool; 128]>>,
) {
    // Safety net: a panic inside the egui draw must never cross baseview's
    // extern "C" timer callback — that aborts the whole DAW. Catch it here so a
    // GUI glitch degrades to a skipped frame instead of crashing the host.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    let frame = egui::Frame::new().fill(BG0).inner_margin(egui::Margin::same(14));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.visuals_mut().override_text_color = Some(egui::Color32::from_gray(225));
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

        // ── Header bar ───────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("◢◣").color(ACCENT).size(18.0));
            ui.heading(
                egui::RichText::new("RAYDRONE").strong().color(egui::Color32::WHITE).size(22.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Bypass toggle.
                let by = params.bypass.value();
                let btn = egui::Button::new(
                    egui::RichText::new(if by { "BYPASSED" } else { "BYPASS" })
                        .size(11.0)
                        .strong()
                        .color(if by { BG0 } else { egui::Color32::from_gray(200) }),
                )
                .fill(if by { ACCENT } else { BG1 })
                .corner_radius(6)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70)));
                if ui.add(btn).clicked() {
                    setter.begin_set_parameter(&params.bypass);
                    setter.set_parameter(&params.bypass, !by);
                    setter.end_set_parameter(&params.bypass);
                }
                ui.label(
                    egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                        .small()
                        .color(egui::Color32::from_gray(90)),
                );
            });
        });
        ui.label(
            egui::RichText::new("STOCHASTIC · RAY-TRACED · DRONE")
                .size(9.0)
                .color(CYAN)
                .weak(),
        );
        ui.add_space(8.0);

        // Scrollable body so no control is ever cut off by the host window size.
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {

        // ── Ray visualizer (shows the live render + autoevolution) ───────────
        draw_visualizer(ui, viz, params.focus.value(), params.evolve.value());
        ui.add_space(10.0);

        // ── Source / character menus (one wide row) ──────────────────────────
        ui.horizontal(|ui| {
            ui.label(menu_tag("SOURCE"));
            let current =
                sample_name.lock().map(|s| s.clone()).unwrap_or_else(|_| "scene".to_string());
            egui::ComboBox::from_id_salt("scene_menu")
                .selected_text(egui::RichText::new(current).color(CYAN))
                .width(150.0)
                .show_ui(ui, |ui| {
                    // Live input: process the track audio into an ambient texture.
                    if ui.selectable_label(false, "◉  Live input (FX)").clicked() {
                        *sample_name.lock().unwrap() = "live input".to_string();
                        *params.sample_path.lock().unwrap() = Some("live".to_string());
                        if let Ok(mut v) = viz.lock() {
                            v.wave = Vec::new();
                        }
                        *pending.lock().unwrap() = Some(SceneCmd::Live(6.0));
                    }
                    ui.separator();
                    for &(label, scene) in &[
                        ("Pad", Scene::Pad),
                        ("Choir", Scene::Choir),
                        ("Bell", Scene::Bell),
                        ("Noise", Scene::Noise),
                    ] {
                        if ui.selectable_label(false, format!("●  {label}")).clicked() {
                            load_scene(scene, pending, viz, sample_name, &params.sample_path);
                        }
                    }
                });
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Load WAV…").color(BG0).size(12.0))
                        .fill(CYAN)
                        .corner_radius(6),
                )
                .clicked()
            {
                spawn_wav_dialog(pending, viz, sample_name, &params.sample_path);
            }
            ui.add_space(16.0);
            ui.label(menu_tag("CHARACTER"));
            egui::ComboBox::from_id_salt("character_menu")
                .selected_text(egui::RichText::new("Presets…").color(ACCENT))
                .width(150.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(false, "Tonal — narrow, pitched").clicked() {
                        apply_preset(setter, params, Preset::Tonal);
                    }
                    if ui.selectable_label(false, "Drone — wide, dense").clicked() {
                        apply_preset(setter, params, Preset::Drone);
                    }
                    if ui.selectable_label(false, "Shimmer — bright, evolving").clicked() {
                        apply_preset(setter, params, Preset::Shimmer);
                    }
                });
        });

        ui.add_space(8.0);

        // ── Knob panels in a wide row (landscape, pro layout) ────────────────
        ui.horizontal_top(|ui| {
            knob_group(ui, "RENDER", |ui| {
                knob(ui, &params.density, setter, "DENSITY", ACCENT);
                knob(ui, &params.aperture, setter, "APERTURE", ACCENT);
                knob(ui, &params.focus, setter, "FOCUS", ACCENT);
            });
            knob_group(ui, "MOTION", |ui| {
                knob(ui, &params.evolve, setter, "EVOLVE", CYAN);
                knob(ui, &params.shimmer, setter, "SHIMMER", CYAN);
                knob(ui, &params.keymix, setter, "KEYS", CYAN);
            });
            knob_group(ui, "RECURSIVE RAYS", |ui| {
                knob(ui, &params.bounce, setter, "BOUNCE", ACCENT);
                knob(ui, &params.reflect, setter, "REFLECT", ACCENT);
            });
            knob_group(ui, "OUTPUT", |ui| {
                knob(ui, &params.reverb, setter, "REVERB", ACCENT);
                knob(ui, &params.mix, setter, "MIX", CYAN);
                knob(ui, &params.master, setter, "MASTER", ACCENT);
            });
        });

        ui.add_space(8.0);
        // PLAY header with octave shift (affects the piano + computer-keyboard row).
        let oct_id = egui::Id::new("raydrone_octave");
        let mut octave: i32 = ui.ctx().data(|d| d.get_temp(oct_id)).unwrap_or(0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PLAY  ·  keys / mouse / MIDI")
                    .size(10.0)
                    .strong()
                    .color(egui::Color32::from_rgb(150, 150, 175)),
            );
            ui.add_space(8.0);
            if ui.add(egui::Button::new("  OCT −  ").fill(BG1).corner_radius(6)).clicked() {
                octave = (octave - 1).max(-3);
            }
            ui.label(egui::RichText::new(format!("{octave:+}")).color(CYAN).strong());
            if ui.add(egui::Button::new("  OCT +  ").fill(BG1).corner_radius(6)).clicked() {
                octave = (octave + 1).min(3);
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(oct_id, octave));
        draw_piano(ui, keys, octave);

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "FX on a track: SOURCE ▸ Live input + MIX (dry↔drone) · hold notes to play chords.",
            )
            .size(10.0)
            .color(egui::Color32::from_gray(120)),
        );
        }); // ScrollArea
    });
    })); // catch_unwind
}

/// The signature view: a focal "camera" casting rays at the scene timeline. The
/// dispersion cone is the depth of field; the rays land where grains are fired;
/// everything glows with the live output level and drifts with autoevolution.
fn draw_visualizer(ui: &mut egui::Ui, viz: &Viz, base_focus: f32, evolve: f32) {
    let desired = egui::vec2(ui.available_width(), 184.0);
    let (rect, _resp) = ui.allocate_exact_size(desired, egui::Sense::hover());
    // Keep the rays alive even when the host isn't repainting for us.
    ui.ctx().request_repaint();
    let p = ui.painter_at(rect);

    let snap = viz.lock().map(|v| v.clone()).unwrap_or_default();
    let t = ui.input(|i| i.time) as f32;

    let rounding = egui::CornerRadius::same(10);
    p.rect_filled(rect, rounding, BG1);
    p.rect_stroke(
        rect,
        rounding,
        egui::Stroke::new(1.0, egui::Color32::from_gray(34)),
        egui::StrokeKind::Inside,
    );

    let pad = 14.0;
    let left = rect.left() + pad;
    let width = rect.width() - 2.0 * pad;
    let base_y = rect.bottom() - 26.0;
    let focus_x = left + snap.focus.clamp(0.0, 1.0) * width;
    let base_focus_x = left + base_focus.clamp(0.0, 1.0) * width;
    let apex_y = rect.top() + 30.0;
    let apex = egui::pos2(focus_x, apex_y);
    let level = snap.level.clamp(0.0, 1.0);
    let glow = (level * 6.0).clamp(0.0, 1.0); // perceptual lift

    // ── Recursive autoevolution: the band the focus wanders within, plus a
    // feedback orbit around the apex. This is the visible signature of the
    // output looping back into focus & aperture.
    if evolve > 0.001 {
        let hr = (0.225 * evolve * width).min(width * 0.5);
        let band = egui::Rect::from_min_max(
            egui::pos2((base_focus_x - hr).max(left), apex_y - 6.0),
            egui::pos2((base_focus_x + hr).min(left + width), apex_y + 6.0),
        );
        p.rect_filled(
            band,
            egui::CornerRadius::same(6),
            egui::Color32::from_rgba_unmultiplied(40, 230, 220, (16.0 + glow * 26.0) as u8),
        );
        // Feedback orbit (recursion): a dot circling the apex.
        let orbit_r = 9.0 + evolve * 9.0;
        let ang = t * (0.6 + evolve * 2.2);
        let od = egui::pos2(apex.x + orbit_r * ang.cos(), apex.y + orbit_r * ang.sin() * 0.55);
        p.circle_stroke(
            apex,
            orbit_r,
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(40, 230, 220, 60)),
        );
        p.circle_filled(od, 2.0, CYAN);
        p.text(
            egui::pos2(left, apex_y - 14.0),
            egui::Align2::LEFT_BOTTOM,
            "↻ AUTOEVOLUTION",
            egui::FontId::proportional(9.0),
            egui::Color32::from_rgba_unmultiplied(40, 230, 220, 180),
        );
    }

    // Scene waveform along the baseline.
    if !snap.wave.is_empty() {
        let n = snap.wave.len();
        for (i, &peak) in snap.wave.iter().enumerate() {
            let x = left + (i as f32 / (n - 1).max(1) as f32) * width;
            let h = peak.clamp(0.0, 1.0) * 18.0;
            p.line_segment(
                [egui::pos2(x, base_y - h), egui::pos2(x, base_y + h)],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 70, 80)),
            );
        }
    }
    // Baseline.
    p.line_segment(
        [egui::pos2(left, base_y), egui::pos2(left + width, base_y)],
        egui::Stroke::new(1.0, egui::Color32::from_gray(55)),
    );

    // Dispersion cone (depth of field) around the focus.
    let ap_half = (snap.aperture.clamp(0.0, 1.0) * width).max(2.0);
    let cone = egui::Color32::from_rgba_unmultiplied(255, 53, 94, (26.0 + glow * 40.0) as u8);
    p.add(egui::Shape::convex_polygon(
        vec![
            apex,
            egui::pos2((focus_x - ap_half).max(left), base_y),
            egui::pos2((focus_x + ap_half).min(left + width), base_y),
        ],
        cone,
        egui::Stroke::NONE,
    ));

    // Rays: newest are brightest. Each ray = a line from the focal apex to where
    // its grain landed on the timeline.
    let n = snap.rays.len();
    for i in 0..n {
        let pos = snap.rays[i].clamp(0.0, 1.0);
        // Age from the ring write head (0 = newest).
        let age = (snap.ray_w + n - 1 - i) % n;
        let a = 1.0 - (age as f32 / n as f32);
        if a <= 0.02 {
            continue;
        }
        let x = left + pos * width;
        let landing = egui::pos2(x, base_y);
        // Hue: near the focus → accent, far → cyan.
        let d = ((x - focus_x).abs() / width.max(1.0)).clamp(0.0, 1.0);
        let col = lerp_color(ACCENT, CYAN, d);
        let alpha = (a * a * (90.0 + glow * 120.0)) as u8;
        let c = egui::Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha);
        p.line_segment([apex, landing], egui::Stroke::new(0.8 + level * 2.0, c));
        // Landing spark.
        p.circle_filled(landing, 1.0 + a * 1.6, c);
    }

    // Focal apex bloom + focus column.
    let pulse = 0.5 + 0.5 * (t * 2.0).sin();
    let bloom = 6.0 + glow * 16.0 + pulse * 2.0;
    p.circle_filled(
        apex,
        bloom,
        egui::Color32::from_rgba_unmultiplied(255, 53, 94, (30.0 + glow * 60.0) as u8),
    );
    p.circle_filled(apex, 2.2, egui::Color32::WHITE);
    p.line_segment(
        [apex, egui::pos2(focus_x, base_y)],
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 60)),
    );

    // Output level meter (top-right).
    let meter_w = 60.0;
    let mx = rect.right() - pad - meter_w;
    let my = rect.top() + 14.0;
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(mx, my), egui::vec2(meter_w, 4.0)),
        egui::CornerRadius::same(2),
        egui::Color32::from_gray(40),
    );
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(mx, my), egui::vec2(meter_w * glow, 4.0)),
        egui::CornerRadius::same(2),
        CYAN,
    );
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    egui::Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Downsample a sample to `n` normalized peak magnitudes for the timeline.
fn wave_peaks(samples: &[f32], n: usize) -> Vec<f32> {
    if samples.is_empty() || n == 0 {
        return Vec::new();
    }
    let mut out = vec![0.0f32; n];
    let mut max = 1e-6f32;
    for (b, slot) in out.iter_mut().enumerate() {
        let start = b * samples.len() / n;
        let end = ((b + 1) * samples.len() / n).min(samples.len());
        let mut peak = 0.0f32;
        for &v in &samples[start..end.max(start)] {
            let a = v.abs();
            if a > peak {
                peak = a;
            }
        }
        *slot = peak;
        if peak > max {
            max = peak;
        }
    }
    for v in out.iter_mut() {
        *v /= max;
    }
    out
}

// ── Built-in scenes ─────────────────────────────────────────────────────────
// Synthesized "scenes" so the plugin makes sound with no WAV loaded. Each is a
// pitched/textured buffer the granular engine renders into a drone.
#[derive(Clone, Copy)]
enum Scene {
    Pad,
    Choir,
    Bell,
    Noise,
}

fn scene_id(scene: Scene) -> &'static str {
    match scene {
        Scene::Pad => "Pad",
        Scene::Choir => "Choir",
        Scene::Bell => "Bell",
        Scene::Noise => "Noise",
    }
}

fn scene_from_id(id: &str) -> Option<Scene> {
    match id {
        "Pad" => Some(Scene::Pad),
        "Choir" => Some(Scene::Choir),
        "Bell" => Some(Scene::Bell),
        "Noise" => Some(Scene::Noise),
        _ => None,
    }
}

/// Generate ~3 s of audio for a built-in scene. Returns (samples, sample_rate).
fn builtin_scene(scene: Scene) -> (Vec<f32>, f32) {
    let sr = 44_100.0f32;
    let len = (sr * 3.0) as usize;
    let mut buf = vec![0.0f32; len];
    let tau = std::f32::consts::TAU;

    match scene {
        // Rich harmonic pad around A2 — a warm, sustained drone source.
        Scene::Pad => {
            let f0 = 110.0;
            for k in 0..8u32 {
                let h = k as f32 + 1.0;
                let amp = 0.6 / h;
                let det = 1.0 + 0.0015 * k as f32; // gentle stretch → lush
                for (i, s) in buf.iter_mut().enumerate() {
                    let t = i as f32 / sr;
                    *s += amp * (tau * f0 * h * det * t).sin();
                }
            }
        }
        // Vowel-ish formant tone around D3 with a slow vibrato.
        Scene::Choir => {
            let f0 = 146.83;
            for k in 0..12u32 {
                let h = k as f32 + 1.0;
                let fr = f0 * h;
                let amp = formant(fr) * 0.5 / h.sqrt();
                for (i, s) in buf.iter_mut().enumerate() {
                    let t = i as f32 / sr;
                    let vib = 1.0 + 0.004 * (tau * 5.0 * t).sin();
                    *s += amp * (tau * fr * vib * t).sin();
                }
            }
        }
        // Inharmonic bell partials, struck a few times across the buffer.
        Scene::Bell => {
            let f0 = 220.0;
            let ratios = [1.0f32, 2.0, 2.4, 3.0, 4.5, 5.33, 6.67];
            let decays = [2.5f32, 2.0, 1.6, 1.3, 1.0, 0.8, 0.6];
            for strike in 0..3 {
                let t0 = strike as f32;
                for (r, d) in ratios.iter().zip(decays.iter()) {
                    let fr = f0 * r;
                    let amp = 0.5 / r;
                    for (i, s) in buf.iter_mut().enumerate() {
                        let t = i as f32 / sr - t0;
                        if t < 0.0 {
                            continue;
                        }
                        *s += amp * (-t / d).exp() * (tau * fr * t).sin();
                    }
                }
            }
        }
        // Colored (low-passed) noise — an airy wash for pure-texture drones.
        Scene::Noise => {
            let mut rng = 0x9e37_79b9u32;
            let mut lp = 0.0f32;
            for s in buf.iter_mut() {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let white = (rng as f32) * (2.0 / 4_294_967_296.0) - 1.0;
                lp += 0.02 * (white - lp);
                *s = lp * 3.0;
            }
        }
    }

    // Normalize to ~0.9 peak.
    let mut max = 1e-6f32;
    for &v in &buf {
        let a = v.abs();
        if a > max {
            max = a;
        }
    }
    let g = 0.9 / max;
    for v in buf.iter_mut() {
        *v *= g;
    }
    (buf, sr)
}

/// Two resonances (~700 & ~1220 Hz) → a rough vowel coloring for the choir scene.
fn formant(f: f32) -> f32 {
    let r = |c: f32, bw: f32| (bw * bw) / ((f - c) * (f - c) + bw * bw);
    0.4 + r(700.0, 120.0) + 0.7 * r(1220.0, 150.0)
}

/// Hand a built-in scene to the audio thread (GUI thread, instant — no dialog).
fn load_scene(
    scene: Scene,
    pending: &PendingSample,
    viz: &Viz,
    sample_name: &Arc<Mutex<String>>,
    sample_path: &Arc<Mutex<Option<String>>>,
) {
    let (data, sr) = builtin_scene(scene);
    let peaks = wave_peaks(&data, 256);
    *sample_name.lock().unwrap() = format!("built-in: {}", scene_id(scene));
    // Persist the choice so the DAW project recalls this scene (not a file path).
    *sample_path.lock().unwrap() = Some(format!("builtin:{}", scene_id(scene)));
    if let Ok(mut v) = viz.lock() {
        v.wave = peaks;
    }
    *pending.lock().unwrap() = Some(SceneCmd::Wav(data, sr));
}

/// Small left-hand tag for a menu row.
fn menu_tag(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(10.0).strong().color(egui::Color32::from_gray(120))
}

/// Polyline arc helper (egui has no arc primitive).
fn paint_arc(p: &egui::Painter, c: egui::Pos2, r: f32, a0: f32, a1: f32, stroke: egui::Stroke) {
    const STEPS: usize = 40;
    let pts: Vec<egui::Pos2> = (0..=STEPS)
        .map(|i| {
            let a = a0 + (a1 - a0) * (i as f32 / STEPS as f32);
            egui::pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, stroke));
}

/// A rotary knob bound to a plugin parameter. Vertical drag changes the value
/// (Shift = fine), double-click resets to default, hover shows the value.
fn knob(ui: &mut egui::Ui, param: &FloatParam, setter: &ParamSetter, label: &str, accent: egui::Color32) {
    let diameter = 52.0;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(diameter + 8.0, diameter + 22.0), egui::Sense::click_and_drag());
    let center = egui::pos2(rect.center().x, rect.top() + diameter * 0.5 + 2.0);
    let radius = diameter * 0.5;

    let mut norm = param.unmodulated_normalized_value();
    if resp.drag_started() {
        setter.begin_set_parameter(param);
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        let speed = if ui.input(|i| i.modifiers.shift) { 0.0009 } else { 0.005 };
        norm = (norm - d.y * speed + d.x * speed * 0.25).clamp(0.0, 1.0);
        setter.set_parameter_normalized(param, norm);
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if resp.double_clicked() {
        let def = param.default_normalized_value();
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, def);
        setter.end_set_parameter(param);
        norm = def;
    }

    let p = ui.painter_at(rect);
    let a0 = 135f32.to_radians();
    let a1 = 405f32.to_radians();
    let av = a0 + (a1 - a0) * norm;
    // Track, value arc, body, pointer.
    paint_arc(&p, center, radius, a0, a1, egui::Stroke::new(3.0, egui::Color32::from_gray(42)));
    paint_arc(&p, center, radius, a0, av, egui::Stroke::new(3.0, accent));
    p.circle_filled(center, radius - 5.0, egui::Color32::from_rgb(22, 22, 32));
    p.circle_stroke(center, radius - 5.0, egui::Stroke::new(1.0, egui::Color32::from_gray(58)));
    if resp.hovered() {
        p.circle_stroke(center, radius - 5.0, egui::Stroke::new(1.0, accent));
    }
    let inner = egui::pos2(center.x + 5.0 * av.cos(), center.y + 5.0 * av.sin());
    let outer = egui::pos2(center.x + (radius - 8.0) * av.cos(), center.y + (radius - 8.0) * av.sin());
    p.line_segment([inner, outer], egui::Stroke::new(2.5, egui::Color32::WHITE));

    // Caption: name normally, live value while hovered/dragged.
    let caption = if resp.hovered() || resp.dragged() {
        param.normalized_value_to_string(norm, true)
    } else {
        label.to_string()
    };
    p.text(
        egui::pos2(center.x, rect.bottom() - 1.0),
        egui::Align2::CENTER_BOTTOM,
        caption,
        egui::FontId::proportional(10.0),
        egui::Color32::from_gray(190),
    );
}

/// A titled, bordered panel holding a row of knobs (the pro-plugin look).
fn knob_group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(BG1)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(34)))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .size(10.0)
                        .strong()
                        .color(egui::Color32::from_gray(150)),
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| add(ui));
            });
        });
}

/// On-screen piano. Plays via three inputs at once: the computer keyboard
/// (A,W,S,E,D,F,T,G,Y,H,U,J,K… from C4), mouse clicks, and incoming MIDI. The
/// resulting note mask is shared with the audio thread, which pitches the rays.
fn draw_piano(ui: &mut egui::Ui, keys: &Arc<Mutex<[bool; 128]>>, octave: i32) {
    // The visible range and the computer-keyboard base both shift with `octave`.
    let lo = (48 + octave * 12).clamp(0, 100); // C3 at octave 0
    let hi = (lo + 28).min(127); // ~2⅓ octaves
    let kb_base = (60 + octave * 12).clamp(0, 115); // C4 at octave 0
    let h = 78.0;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);

    let is_black = |n: i32| matches!(((n % 12) + 12) % 12, 1 | 3 | 6 | 8 | 10);
    let whites: Vec<i32> = (lo..hi).filter(|&n| !is_black(n)).collect();
    let ww = rect.width() / whites.len().max(1) as f32;
    let white_rect = |i: usize| {
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + i as f32 * ww, rect.top()),
            egui::vec2(ww - 1.0, h),
        )
    };
    let black_rect = |i: usize| {
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + (i as f32 + 1.0) * ww - ww * 0.3, rect.top()),
            egui::vec2(ww * 0.6, h * 0.58),
        )
    };

    let mut mask = [false; 128];
    let mut set = |n: i32| {
        if (0..128).contains(&n) {
            mask[n as usize] = true;
        }
    };

    // Computer keyboard → notes (row from C, base follows the octave).
    const KB: &[(egui::Key, i32)] = &[
        (egui::Key::A, 0), (egui::Key::W, 1), (egui::Key::S, 2), (egui::Key::E, 3),
        (egui::Key::D, 4), (egui::Key::F, 5), (egui::Key::T, 6), (egui::Key::G, 7),
        (egui::Key::Y, 8), (egui::Key::H, 9), (egui::Key::U, 10), (egui::Key::J, 11),
        (egui::Key::K, 12), (egui::Key::O, 13), (egui::Key::L, 14),
    ];
    ui.input(|i| {
        for &(k, off) in KB {
            if i.key_down(k) {
                set(kb_base + off);
            }
        }
    });

    // Mouse → the note under a pressed pointer (black keys take priority).
    if resp.is_pointer_button_down_on() {
        if let Some(pp) = ui.input(|i| i.pointer.interact_pos()) {
            let mut hit = None;
            for (i, &n) in whites.iter().enumerate() {
                if is_black(n + 1) && black_rect(i).contains(pp) {
                    hit = Some(n + 1);
                    break;
                }
            }
            if hit.is_none() {
                for (i, &n) in whites.iter().enumerate() {
                    if white_rect(i).contains(pp) {
                        hit = Some(n);
                        break;
                    }
                }
            }
            if let Some(n) = hit {
                set(n);
            }
        }
    }

    // Draw white keys (label every C), then black keys on top.
    let low = egui::CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 };
    for (i, &n) in whites.iter().enumerate() {
        let r = white_rect(i);
        let col = if mask[n as usize] { CYAN } else { egui::Color32::from_gray(232) };
        p.rect_filled(r, low, col);
        p.rect_stroke(r, low, egui::Stroke::new(1.0, egui::Color32::from_gray(40)), egui::StrokeKind::Inside);
        if n % 12 == 0 {
            p.text(
                egui::pos2(r.center().x, r.bottom() - 3.0),
                egui::Align2::CENTER_BOTTOM,
                format!("C{}", n / 12 - 1),
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(90),
            );
        }
    }
    for (i, &n) in whites.iter().enumerate() {
        if is_black(n + 1) {
            let col = if mask[(n + 1) as usize] { ACCENT } else { egui::Color32::from_gray(16) };
            p.rect_filled(black_rect(i), low, col);
        }
    }

    if let Ok(mut k) = keys.lock() {
        *k = mask;
    }
}

/// Open the native WAV dialog on a background thread (see the crash note above).
fn spawn_wav_dialog(
    pending: &PendingSample,
    viz: &Viz,
    sample_name: &Arc<Mutex<String>>,
    sample_path: &Arc<Mutex<Option<String>>>,
) {
    let pending = pending.clone();
    let viz = viz.clone();
    let sample_name = sample_name.clone();
    let sample_path = sample_path.clone();
    std::thread::spawn(move || {
        if let Some(file) = rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).pick_file() {
            match load_wav(&file) {
                Ok((data, sr)) => {
                    let peaks = wave_peaks(&data, 256);
                    *sample_name.lock().unwrap() = file
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "loaded".to_string());
                    *sample_path.lock().unwrap() = Some(file.to_string_lossy().to_string());
                    if let Ok(mut v) = viz.lock() {
                        v.wave = peaks;
                    }
                    *pending.lock().unwrap() = Some(SceneCmd::Wav(data, sr));
                }
                Err(e) => {
                    *sample_name.lock().unwrap() = format!("error: {e}");
                }
            }
        }
    });
}

// ── Presets ("Simple mode") ─────────────────────────────────────────────────
#[derive(Clone, Copy)]
enum Preset {
    Tonal,
    Drone,
    Shimmer,
}

/// Set one parameter as a complete host gesture (so DAW automation records it).
fn set_p(setter: &ParamSetter, param: &FloatParam, value: f32) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

/// Move the character knobs (density + aperture + reverb + evolve + shimmer) to a
/// preset. Focus and Master are left to the user. Mirrors the classic Simple mode.
fn apply_preset(setter: &ParamSetter, params: &Arc<RayDroneParams>, preset: Preset) {
    // (density, aperture_ms, reverb, evolve, shimmer)
    let (d, a, rev, evo, shim) = match preset {
        Preset::Tonal => (140.0, 6.0, 0.10, 0.10, 0.0),
        Preset::Drone => (320.0, 700.0, 0.35, 0.45, 0.0),
        Preset::Shimmer => (420.0, 220.0, 0.50, 0.60, 0.5),
    };
    set_p(setter, &params.density, d);
    set_p(setter, &params.aperture, a);
    set_p(setter, &params.reverb, rev);
    set_p(setter, &params.evolve, evo);
    set_p(setter, &params.shimmer, shim);
}

// ── WAV loading ─────────────────────────────────────────────────────────────
fn load_wav(path: &Path) -> Result<(Vec<f32>, f32), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let sr = spec.sample_rate as f32;
    let ch = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect()
        }
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| (s.unwrap_or(0) as f32) / max).collect()
        }
    };

    // Downmix to mono (the engine works on a single-channel scene).
    let mono: Vec<f32> = if ch <= 1 {
        interleaved
    } else {
        interleaved
            .chunks(ch)
            .map(|fr| fr.iter().sum::<f32>() / (ch as f32))
            .collect()
    };

    if mono.len() < 2 {
        return Err("empty or too short".to_string());
    }
    Ok((mono, sr))
}

fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

impl ClapPlugin for RayDrone {
    const CLAP_ID: &'static str = "com.raydrone.simple";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Stochastic ray-traced granular ambient processor / drone");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Reverb,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for RayDrone {
    const VST3_CLASS_ID: [u8; 16] = *b"RayDroneSimplVST";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Reverb];
}

nih_export_clap!(RayDrone);
nih_export_vst3!(RayDrone);
