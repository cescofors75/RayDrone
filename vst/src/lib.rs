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
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};

mod engine;
use engine::{Engine, VLOG};

/// Source samples + native sample-rate waiting to be picked up by the audio thread.
type PendingSample = Arc<Mutex<Option<(Vec<f32>, f32)>>>;

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
    /// Master output level.
    #[id = "master"]
    master: FloatParam,
}

impl Default for RayDrone {
    fn default() -> Self {
        Self {
            params: Arc::new(RayDroneParams::default()),
            engine: Engine::new(44100.0),
            pending: Arc::new(Mutex::new(None)),
            sample_name: Arc::new(Mutex::new("(no sample)".to_string())),
            viz: Arc::new(Mutex::new(VizState::default())),
        }
    }
}

impl Default for RayDroneParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(440, 680),
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

    // Instrument: no audio input, stereo output.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
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
        let egui_state = self.params.editor_state.clone();

        create_egui_editor(
            egui_state,
            (),
            |_, _| {},
            move |ctx, setter, _state| {
                draw_ui(ctx, setter, &params, &pending, &sample_name, &viz);
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
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Pick up a freshly-loaded sample from the GUI thread (rare event).
        if let Ok(mut slot) = self.pending.try_lock() {
            if let Some((data, sr)) = slot.take() {
                self.engine.load(data, sr);
            }
        }

        // Push current parameter values to the engine.
        self.engine.set_density(self.params.density.value());
        self.engine.set_aperture_ms(self.params.aperture.value());
        self.engine.set_focus(self.params.focus.value());
        self.engine.set_reverb(self.params.reverb.value());
        self.engine.set_feedback(self.params.evolve.value());
        self.engine.set_octave(self.params.shimmer.value());

        for mut frame in buffer.iter_samples() {
            self.engine.set_master(self.params.master.smoothed.next());
            let (l, r) = self.engine.tick();
            let mut ch = frame.iter_mut();
            if let Some(s) = ch.next() {
                *s = l;
            }
            if let Some(s) = ch.next() {
                *s = r;
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
) {
    let frame = egui::Frame::new().fill(BG0).inner_margin(egui::Margin::same(14));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.visuals_mut().override_text_color = Some(egui::Color32::from_gray(225));

        ui.vertical_centered(|ui| {
            ui.heading(egui::RichText::new("RAYDRONE").strong().color(ACCENT).size(26.0));
            ui.label(
                egui::RichText::new("stochastic ray-traced drone")
                    .italics()
                    .color(egui::Color32::from_gray(140)),
            );
        });
        ui.add_space(10.0);

        // ── Ray visualizer ──────────────────────────────────────────────────
        draw_visualizer(ui, viz);
        ui.add_space(10.0);

        if ui
            .add(egui::Button::new(egui::RichText::new("  Load WAV…  ").color(BG0)).fill(CYAN))
            .clicked()
        {
            // IMPORTANT: open the native file dialog on a SEPARATE THREAD. Calling
            // the blocking `rfd` dialog directly inside the egui/baseview event loop
            // reenters the host's run loop and crashes some DAWs (notably Reaper on
            // macOS). Off-thread, the result is handed back through the shared slots.
            let pending = pending.clone();
            let viz = viz.clone();
            let sample_name = sample_name.clone();
            let sample_path = params.sample_path.clone();
            std::thread::spawn(move || {
                if let Some(file) =
                    rfd::FileDialog::new().add_filter("WAV audio", &["wav"]).pick_file()
                {
                    match load_wav(&file) {
                        Ok((data, sr)) => {
                            let peaks = wave_peaks(&data, 256);
                            *sample_name.lock().unwrap() = file
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "loaded".to_string());
                            *sample_path.lock().unwrap() =
                                Some(file.to_string_lossy().to_string());
                            if let Ok(mut v) = viz.lock() {
                                v.wave = peaks;
                            }
                            *pending.lock().unwrap() = Some((data, sr));
                        }
                        Err(e) => {
                            *sample_name.lock().unwrap() = format!("error: {e}");
                        }
                    }
                }
            });
        }

        // ── Built-in scenes: instant sound, no file needed ──────────────────
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Built-in:").color(egui::Color32::from_gray(160)));
            for &(label, scene) in &[
                ("Pad", Scene::Pad),
                ("Choir", Scene::Choir),
                ("Bell", Scene::Bell),
                ("Noise", Scene::Noise),
            ] {
                if preset_button(ui, label).clicked() {
                    load_scene(scene, pending, viz, sample_name, &params.sample_path);
                }
            }
        });
        ui.label(
            egui::RichText::new(format!("scene: {}", sample_name.lock().unwrap()))
                .color(egui::Color32::from_gray(150))
                .small(),
        );

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);

        // ── Presets (the "Simple mode": one click sets the character) ────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Character:").color(egui::Color32::from_gray(160)));
            if preset_button(ui, "Tonal").clicked() {
                apply_preset(setter, params, Preset::Tonal);
            }
            if preset_button(ui, "Drone").clicked() {
                apply_preset(setter, params, Preset::Drone);
            }
            if preset_button(ui, "Shimmer").clicked() {
                apply_preset(setter, params, Preset::Shimmer);
            }
        });
        ui.add_space(6.0);

        labeled(ui, "Density", &params.density, setter);
        labeled(ui, "Aperture", &params.aperture, setter);
        labeled(ui, "Focus", &params.focus, setter);
        labeled(ui, "Reverb", &params.reverb, setter);
        labeled(ui, "Evolve", &params.evolve, setter);
        labeled(ui, "Shimmer", &params.shimmer, setter);
        labeled(ui, "Master", &params.master, setter);

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(
                "Narrow aperture → tonal · wide → drone.\n\
                 More density → smoother render · Evolve → it drifts on its own.",
            )
            .color(egui::Color32::from_gray(120))
            .small(),
        );
    });
}

/// The signature view: a focal "camera" casting rays at the scene timeline. The
/// dispersion cone is the depth of field; the rays land where grains are fired;
/// everything glows with the live output level and drifts with autoevolution.
fn draw_visualizer(ui: &mut egui::Ui, viz: &Viz) {
    let desired = egui::vec2(ui.available_width(), 200.0);
    let (rect, _resp) = ui.allocate_exact_size(desired, egui::Sense::hover());
    // Keep the rays alive even when the host isn't repainting for us.
    ui.ctx().request_repaint();
    let p = ui.painter_at(rect);

    let snap = viz.lock().map(|v| v.clone()).unwrap_or_default();
    let t = ui.input(|i| i.time) as f32;

    let rounding = egui::CornerRadius::same(10);
    p.rect_filled(rect, rounding, BG1);

    let pad = 14.0;
    let left = rect.left() + pad;
    let width = rect.width() - 2.0 * pad;
    let base_y = rect.bottom() - 26.0;
    let focus_x = left + snap.focus.clamp(0.0, 1.0) * width;
    let apex = egui::pos2(focus_x, rect.top() + 22.0);
    let level = snap.level.clamp(0.0, 1.0);
    let glow = (level * 6.0).clamp(0.0, 1.0); // perceptual lift

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
    *pending.lock().unwrap() = Some((data, sr));
}

fn labeled(ui: &mut egui::Ui, name: &str, param: &FloatParam, setter: &ParamSetter) {
    ui.label(name);
    ui.add(widgets::ParamSlider::for_param(param, setter));
    ui.add_space(4.0);
}

// ── Presets ("Simple mode") ─────────────────────────────────────────────────
#[derive(Clone, Copy)]
enum Preset {
    Tonal,
    Drone,
    Shimmer,
}

fn preset_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(egui::Color32::from_gray(230)))
            .fill(BG1)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70))),
    )
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
        Some("Stochastic ray-traced granular drone from a WAV scene");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for RayDrone {
    const VST3_CLASS_ID: [u8; 16] = *b"RayDroneSimplVST";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(RayDrone);
nih_export_vst3!(RayDrone);
