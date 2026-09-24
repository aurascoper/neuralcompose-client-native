mod recording;
mod renderer;
mod source;
use eframe::{egui, egui_wgpu};
use neuralcompose_hypnagogic::{
    turn_log::TurnLine,
    visual::{TurnVisual, VisualPacket},
};
use neuralcompose_mobile_core::{StreamMonitor, StreamPhase};
use neuralcompose_viz::{
    semantic,
    signal::{self, Config, Geometry, Mapping},
};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

struct Args {
    url: String,
    demo: bool,
    replay: Option<PathBuf>,
    fixture: String,
    config: Config,
    socket: Option<PathBuf>,
    context: Option<PathBuf>,
    metrics: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    seconds: f64,
    warmup: f64,
    repeat: u32,
    full: bool,
    recorded: bool,
}
impl Args {
    fn parse() -> Result<Self, String> {
        let mut a = Self {
            url: "ws://127.0.0.1:8788/api/eeg/stream".into(),
            demo: false,
            replay: None,
            fixture: "rhythmic".into(),
            config: Config::default(),
            socket: None,
            context: None,
            metrics: None,
            screenshot: None,
            seconds: 0.0,
            warmup: 5.0,
            repeat: 1,
            full: true,
            recorded: false,
        };
        let mut args = std::env::args().skip(1);
        let mut inputs = 0;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--demo" => {
                    a.demo = true;
                    inputs += 1;
                }
                "--trace-only" => a.full = false,
                "--recorded" => a.recorded = true,
                "--help" | "-h" => {
                    println!("neuralcompose-viz [--demo | --eeg-url URL | --replay ID.eeg.jsonl]\n  --fixture rhythmic|noise|impulse  --mapping channels|delay\n  --visualization-socket PATH      --context-log ID.turns.jsonl\n  --trace-only  --seconds N  --warmup N  --repeat N\n  --metrics-out PATH  --recorded   --screenshot PATH\nRecorded runs require a clean release build and clean checkout, 5s warmup + 60s,\nand output outside the source tree. Dirty runs are diagnostic/non-quotable.");
                    std::process::exit(0);
                }
                _ => {
                    let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
                    match arg.as_str() {
                        "--eeg-url" => {
                            a.url = value;
                            inputs += 1;
                        }
                        "--replay" => {
                            a.replay = Some(value.into());
                            inputs += 1;
                        }
                        "--fixture" => a.fixture = value,
                        "--mapping" => {
                            a.config.mapping = match value.as_str() {
                                "channels" => Mapping::Channels,
                                "delay" => Mapping::Delay,
                                _ => return Err("mapping must be channels or delay".into()),
                            }
                        }
                        "--visualization-socket" => a.socket = Some(value.into()),
                        "--context-log" => a.context = Some(value.into()),
                        "--metrics-out" => a.metrics = Some(value.into()),
                        "--screenshot" => a.screenshot = Some(value.into()),
                        "--seconds" => a.seconds = value.parse().map_err(|_| "invalid seconds")?,
                        "--warmup" => a.warmup = value.parse().map_err(|_| "invalid warmup")?,
                        "--repeat" => a.repeat = value.parse().map_err(|_| "invalid repeat")?,
                        _ => return Err(format!("unknown option {arg}")),
                    }
                }
            }
        }
        if inputs > 1 {
            return Err("choose exactly one EEG source".into());
        }
        if !["rhythmic", "noise", "impulse"].contains(&a.fixture.as_str()) {
            return Err("unknown fixture".into());
        }
        if !a.seconds.is_finite() || a.seconds < 0.0 || !a.warmup.is_finite() || a.warmup < 0.0 {
            return Err("durations must be finite and nonnegative".into());
        }
        if a.recorded
            && (!a.demo
                || a.seconds != 60.0
                || a.warmup != 5.0
                || !(1..=3).contains(&a.repeat)
                || a.metrics.is_none()
                || a.screenshot.is_some()
                || a.socket.is_some()
                || a.context.is_some())
        {
            return Err("recorded protocol requires demo, 60 seconds, 5 warmup, repeat 1..3, metrics output, no screenshot".into());
        }
        Ok(a)
    }
}
fn repo() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    path.canonicalize().unwrap_or(path)
}
fn clean_tree() -> bool {
    std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo())
        .output()
        .is_ok_and(|r| r.status.success() && r.stdout.is_empty())
        && std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo())
            .output()
            .is_ok_and(|r| String::from_utf8_lossy(&r.stdout).trim() == env!("NC_VIZ_COMMIT"))
}
#[derive(Clone)]
struct Analysis {
    geometry: Arc<Geometry>,
    ratio: Option<f32>,
    phase: StreamPhase,
    received: u64,
    received_ms: Option<u64>,
    generation: u64,
    rms: [Option<f64>; 4],
}
impl Default for Analysis {
    fn default() -> Self {
        Self {
            geometry: Arc::new(Geometry::default()),
            ratio: None,
            phase: StreamPhase::Connecting,
            received: 0,
            received_ms: None,
            generation: 0,
            rms: [None; 4],
        }
    }
}
struct App {
    args: Args,
    epoch: Instant,
    stop: Arc<AtomicBool>,
    config: Arc<Mutex<Config>>,
    analysis: Arc<Mutex<Analysis>>,
    displayed: Analysis,
    feed: Arc<Mutex<source::SemanticFeed>>,
    history: VecDeque<TurnVisual>,
    context: Vec<TurnLine>,
    session: String,
    sequence: u64,
    recorder: Arc<Mutex<recording::Recorder>>,
    gpu: String,
    sha: String,
    clean: bool,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    paused: bool,
    atmosphere: bool,
    semantics: bool,
    finished: bool,
    screenshot_requested: bool,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Result<Self, String> {
        let clean =
            env!("NC_VIZ_CLEAN") == "true" && env!("NC_VIZ_PROFILE") == "release" && clean_tree();
        if args.recorded && !clean {
            return Err(
                "recorded run refused: build and checkout must both be clean and identical".into(),
            );
        }
        if let Some(path) = &args.metrics {
            let parent = path.parent().unwrap_or(std::path::Path::new("."));
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            if args.recorded
                && parent
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .starts_with(repo())
            {
                return Err("recorded output must be outside checkout".into());
            }
            if path.exists() {
                return Err("metrics output exists; recorded runs cannot be replaced".into());
            }
        }
        let context = match &args.context {
            Some(p) => source::context(p)?,
            None => Vec::new(),
        };
        let input = if args.demo {
            source::Source::Demo(args.fixture.clone())
        } else if let Some(p) = &args.replay {
            source::Source::Replay(source::capture(p)?)
        } else {
            source::Source::Live(args.url.clone())
        };
        let render = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("wgpu renderer unavailable")?;
        let info = render.adapter.get_info();
        let gpu = format!(
            "{}; {:?}; {}; {}",
            info.name, info.backend, info.driver, info.driver_info
        );
        render
            .renderer
            .write()
            .callback_resources
            .insert(renderer::Resources::new(
                &render.device,
                render.target_format,
            ));
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let sha = Sha256::digest(
            std::fs::read(std::env::current_exe().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        )
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
        let epoch = Instant::now();
        let stop = Arc::new(AtomicBool::new(false));
        let monitor = Arc::new(StreamMonitor::with_defaults());
        source::run(input, monitor.clone(), epoch, stop.clone());
        let config = Arc::new(Mutex::new(args.config));
        let analysis = Arc::new(Mutex::new(Analysis::default()));
        {
            let (cfg, out, stop) = (config.clone(), analysis.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut previous = (u64::MAX, Config::default());
                let mut last_band = Instant::now() - Duration::from_secs(1);
                while !stop.load(Ordering::Relaxed) {
                    let snapshot = monitor.signal_snapshot(epoch.elapsed().as_millis() as u64);
                    let c = *cfg.lock().unwrap();
                    let changed = snapshot.received != previous.0 || c != previous.1;
                    let mut next = out.lock().unwrap().clone();
                    if next.generation != snapshot.generation {
                        next = Analysis::default();
                    }
                    next.phase = snapshot.phase;
                    next.received = snapshot.received;
                    next.received_ms = snapshot.received_at_ms;
                    next.generation = snapshot.generation;
                    if changed && !snapshot.samples.is_empty() {
                        next.geometry = Arc::new(signal::geometry(&snapshot.samples, c));
                        next.rms = std::array::from_fn(|ch| {
                            let n = snapshot.samples.len() as f64;
                            let rms = (snapshot
                                .samples
                                .iter()
                                .map(|s| s.channels[ch].powi(2) / n)
                                .sum::<f64>())
                            .sqrt();
                            rms.is_finite().then_some(rms)
                        });
                        previous = (snapshot.received, c);
                    }
                    if last_band.elapsed() >= Duration::from_millis(250) {
                        next.ratio = if matches!(snapshot.phase, StreamPhase::Live) {
                            signal::band_ratio(&snapshot.samples, c.rate)
                        } else {
                            None
                        };
                        last_band = Instant::now();
                    }
                    *out.lock().unwrap() = next;
                    std::thread::sleep(Duration::from_millis(15));
                }
            });
        }
        let feed = Arc::new(Mutex::new(source::SemanticFeed::default()));
        if let Some(path) = &args.socket {
            source::semantic(path.clone(), feed.clone(), stop.clone());
        }
        let recorder = Arc::new(Mutex::new(recording::Recorder::new(
            args.warmup,
            args.seconds,
        )));
        let full = args.full;
        Ok(Self {
            args,
            epoch,
            stop,
            config,
            analysis,
            displayed: Analysis::default(),
            feed,
            history: VecDeque::new(),
            context,
            session: String::new(),
            sequence: 0,
            recorder,
            gpu,
            sha,
            clean,
            yaw: 0.6,
            pitch: 0.3,
            zoom: 3.2,
            paused: false,
            atmosphere: full,
            semantics: full,
            finished: false,
            screenshot_requested: false,
        })
    }
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.stop.store(true, Ordering::Relaxed);
        if let Some(path) = &self.args.metrics {
            let case = format!(
                "{}-{}-{}",
                self.args.fixture,
                if self.args.config.mapping == Mapping::Channels {
                    "channels"
                } else {
                    "delay"
                },
                if self.args.full { "full" } else { "trace" }
            );
            let run = self.recorder.lock().unwrap().finish(
                case,
                self.args.repeat,
                self.gpu.clone(),
                self.sha.clone(),
                self.clean && clean_tree(),
                self.args.recorded,
            );
            use std::io::Write;
            let result = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .and_then(|mut f| {
                    f.write_all(&serde_json::to_vec_pretty(&run).unwrap())?;
                    f.sync_all()
                });
            if let Err(e) = result {
                eprintln!("cannot write measurements: {e}");
            } else {
                println!(
                    "{}",
                    serde_json::json!({"metrics":path,"quotable":run.quotable,"frames":run.rendered_frames,"frame_p95_ms":run.frame_p95_ms,"age_p95_ms":run.age_p95_ms})
                );
            }
        }
    }
    fn ingest_semantics(&mut self) {
        let packet = if self.args.demo && self.args.socket.is_none() {
            let turn = (self.epoch.elapsed().as_secs() / 5).min(10000);
            Some(VisualPacket {
                session: "SYNTHETIC DEMO".into(),
                sequence: turn + 1,
                skipped_notifications: 0,
                state: semantic::demo(turn),
            })
        } else {
            self.feed.lock().unwrap().packet.clone()
        };
        if let Some(packet) = packet {
            if packet.session != self.session {
                self.history.clear();
                self.sequence = 0;
                self.session = packet.session.clone();
            }
            if packet.sequence > self.sequence {
                self.sequence = packet.sequence;
                self.recorder
                    .lock()
                    .unwrap()
                    .turn(self.epoch.elapsed().as_secs_f64());
                if self
                    .history
                    .back()
                    .is_some_and(|s| s.turn == packet.state.turn)
                {
                    self.history.pop_back();
                }
                self.history.push_back(packet.state);
                while self.history.len() > 16 {
                    self.history.pop_front();
                }
            }
        }
    }
    fn semantic_scene(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        let shown: Vec<_> = self.history.iter().rev().take(4).rev().collect();
        if shown.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Dialectic feed unavailable",
                egui::FontId::proportional(16.0),
                egui::Color32::GRAY,
            );
            return;
        }
        let width = rect.width() / 4.0;
        let origin = rect.left_top() + egui::vec2(width * 0.45, rect.height() * 0.30);
        let p = |column: usize, lane: usize| {
            let q = semantic::position(column, lane);
            origin + egui::vec2(q[0] * width, q[1] * 75.0)
        };
        for (column, state) in shown.iter().enumerate() {
            let input = p(column, 0);
            painter.circle_filled(input, 4.0, egui::Color32::from_rgb(195, 212, 227));
            painter.text(
                input + egui::vec2(0.0, -18.0),
                egui::Align2::CENTER_CENTER,
                format!("TURN {} · {}", state.turn, state.stage),
                egui::FontId::monospace(10.0),
                egui::Color32::from_gray(155),
            );
            ui.interact(
                egui::Rect::from_center_size(input, egui::vec2(80.0, 25.0)),
                egui::Id::new(("heard", state.turn)),
                egui::Sense::hover(),
            )
            .on_hover_text(&state.heard);
            for (index, candidate) in state.candidates.iter().enumerate() {
                let node = p(column, index + 1);
                let color = if index == 0 {
                    egui::Color32::from_rgb(98, 224, 231)
                } else {
                    egui::Color32::from_rgb(192, 146, 248)
                };
                painter.circle_filled(node, 5.0, color);
                let weight = state
                    .weights
                    .as_ref()
                    .and_then(|w| w.get(index))
                    .map(|w| format!(" · weight {w:.3}"))
                    .unwrap_or_default();
                painter.text(
                    node + egui::vec2(9.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{}{weight}", candidate.role),
                    egui::FontId::proportional(11.0),
                    color,
                );
                let cosine =
                    semantic::cosine(state.heard_embedding.as_ref(), candidate.embedding.as_ref());
                if let Some(value) = cosine {
                    let bend = input + egui::vec2(24.0 + index as f32 * 12.0, 30.0);
                    painter.add(egui::Shape::line(
                        vec![input, bend, egui::pos2(bend.x, node.y), node],
                        egui::Stroke::new(0.8, color.gamma_multiply(0.45)),
                    ));
                    let mid = node + egui::vec2(0.0, 19.0);
                    painter.text(
                        mid + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("input cos {value:.4}"),
                        egui::FontId::monospace(10.0),
                        color,
                    );
                }
                let text = format!(
                    "{}\nInput cosine: {}\nPosition: turn order and role only",
                    candidate.text,
                    cosine
                        .map(|x| format!("{x:.9}"))
                        .unwrap_or("unavailable".into())
                );
                ui.interact(
                    egui::Rect::from_center_size(node, egui::vec2(width * 0.8, 40.0)),
                    egui::Id::new((state.turn, index)),
                    egui::Sense::hover(),
                )
                .on_hover_text(text);
            }
            if state.candidates.len() >= 2 {
                let value = semantic::cosine(
                    state.candidates[0].embedding.as_ref(),
                    state.candidates[1].embedding.as_ref(),
                );
                if let Some(v) = value {
                    let a = p(column, 1) + egui::vec2(-10.0, 0.0);
                    let b = p(column, 2) + egui::vec2(-10.0, 0.0);
                    painter.line_segment([a, b], egui::Stroke::new(0.8, egui::Color32::GRAY));
                    let response = ui.interact(
                        egui::Rect::from_two_pos(
                            a - egui::vec2(12.0, 0.0),
                            b + egui::vec2(12.0, 0.0),
                        ),
                        egui::Id::new(("pair", state.turn)),
                        egui::Sense::hover(),
                    );
                    response.on_hover_text(format!("Candidate cosine: {v:.9}"));
                }
            }
        }
        let latest = shown.last().unwrap();
        let final_text = if latest.repetition_forced_silence {
            "Silence · repetition guard"
        } else {
            latest
                .final_text
                .as_deref()
                .unwrap_or(if latest.stage == "completed" {
                    "Silence"
                } else {
                    "Decision pending"
                })
        };
        let text_rect = egui::Rect::from_min_size(
            rect.left_bottom() + egui::vec2(30.0, -90.0),
            egui::vec2(rect.width() - 60.0, 80.0),
        );
        let galley = painter.layout(
            final_text.to_string(),
            egui::FontId::proportional(22.0),
            egui::Color32::from_rgb(232, 236, 245),
            text_rect.width(),
        );
        painter.galley(text_rect.min, galley, egui::Color32::WHITE);
    }
}
impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.args.seconds > 0.0
            && self.epoch.elapsed().as_secs_f64() > self.args.warmup + self.args.seconds + 0.15
        {
            self.finish();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.ingest_semantics();
        if !self.paused {
            self.displayed = self.analysis.lock().unwrap().clone();
        }
        let now = self.epoch.elapsed().as_secs_f64();
        if self.args.seconds > 0.0 && now > self.args.warmup + self.args.seconds + 0.15 {
            self.finish();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                if let Some(path) = &self.args.screenshot {
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        path,
                        &bytes,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("screenshot: {e}");
                    }
                }
            }
        }
        if self.args.screenshot.is_some() && !self.screenshot_requested && now > 3.0 {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(9, 13, 23))
                    .inner_margin(20.0),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("NeuralCompose");
                    ui.label(
                        egui::RichText::new("PHASE SPACE / DIALECTIC")
                            .color(egui::Color32::from_gray(140)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(if self.args.demo {
                            "SYNTHETIC DEMO"
                        } else if self.args.replay.is_some() {
                            "VERIFIED REPLAY"
                        } else {
                            "EEG STREAM"
                        });
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{:?} · generation {} · {} samples",
                        self.displayed.phase, self.displayed.generation, self.displayed.received
                    ));
                    ui.separator();
                    ui.label(format!(
                        "Alpha / (alpha + theta): {}",
                        self.displayed
                            .ratio
                            .map(|r| format!("{r:.3}"))
                            .unwrap_or("unavailable".into())
                    ));
                    if self.paused {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "DISPLAY PAUSED · ingestion continues",
                        );
                    }
                });
                if self.args.socket.is_some() {
                    let feed = self.feed.lock().unwrap();
                    if !feed.connected || feed.error.is_some() {
                        ui.colored_label(
                            egui::Color32::from_rgb(215, 170, 100),
                            format!(
                                "Dialectic unavailable · cached context · {}",
                                feed.error.as_deref().unwrap_or("disconnected")
                            ),
                        );
                    }
                }
                ui.add_enabled_ui(!self.args.recorded, |ui| {
                    ui.horizontal(|ui| {
                        let mut cfg = self.config.lock().unwrap();
                        ui.selectable_value(&mut cfg.mapping, Mapping::Channels, "4 channels");
                        ui.selectable_value(&mut cfg.mapping, Mapping::Delay, "Delay view");
                        if cfg.mapping == Mapping::Delay {
                            egui::ComboBox::from_id_salt("channel")
                                .selected_text(["TP9", "AF7", "AF8", "TP10"][cfg.channel])
                                .show_ui(ui, |ui| {
                                    for (i, name) in
                                        ["TP9", "AF7", "AF8", "TP10"].iter().enumerate()
                                    {
                                        ui.selectable_value(&mut cfg.channel, i, *name);
                                    }
                                });
                            ui.add(
                                egui::DragValue::new(&mut cfg.delay)
                                    .range(1..=128)
                                    .prefix("τ samples "),
                            );
                            ui.label(format!("{:.2} ms", cfg.delay as f64 / cfg.rate * 1000.0));
                        }
                        ui.add(
                            egui::DragValue::new(&mut cfg.scale)
                                .range(1.0..=2000.0)
                                .prefix("µV/unit "),
                        );
                        ui.checkbox(&mut self.paused, "Pause");
                        ui.checkbox(&mut self.atmosphere, "Bands");
                        ui.checkbox(&mut self.semantics, "Dialectic");
                        if ui.button("Reset view").clicked() {
                            self.yaw = 0.6;
                            self.pitch = 0.3;
                            self.zoom = 3.2;
                        }
                    })
                });
                let rect = ui.available_rect_before_wrap();
                let interaction = ui.allocate_rect(rect, egui::Sense::drag());
                if !self.args.recorded {
                    if interaction.dragged() {
                        let d = ctx.input(|i| i.pointer.delta());
                        self.yaw += d.x * 0.006;
                        self.pitch = (self.pitch + d.y * 0.006).clamp(-1.4, 1.4);
                    }
                    if interaction.hovered() {
                        self.zoom = (self.zoom - ctx.input(|i| i.smooth_scroll_delta.y) * 0.005)
                            .clamp(1.2, 12.0);
                    }
                }
                let rotation =
                    glam::Mat4::from_rotation_x(self.pitch) * glam::Mat4::from_rotation_y(self.yaw);
                let mvp = glam::Mat4::perspective_rh(
                    0.8,
                    rect.width() / rect.height().max(1.0),
                    0.1,
                    100.0,
                ) * glam::Mat4::from_translation(glam::vec3(0.0, 0.0, -self.zoom))
                    * rotation;
                let live = matches!(self.displayed.phase, StreamPhase::Live) && !self.paused;
                let current = self.analysis.lock().unwrap().clone();
                let skipped = self
                    .feed
                    .lock()
                    .unwrap()
                    .packet
                    .as_ref()
                    .map_or(0, |p| p.skipped_notifications);
                let scene = renderer::Scene {
                    geometry: self.displayed.geometry.clone(),
                    uniform: renderer::Uniform {
                        mvp: mvp.to_cols_array_2d(),
                        params: [
                            if live { 1.0 } else { 0.22 },
                            self.displayed.ratio.unwrap_or(0.5),
                            now as f32,
                            0.0,
                        ],
                    },
                    size: [
                        (rect.width() * ctx.pixels_per_point()).max(1.0) as u32,
                        (rect.height() * ctx.pixels_per_point()).max(1.0) as u32,
                    ],
                    particles: self.atmosphere && self.displayed.ratio.is_some(),
                    recorder: self.recorder.clone(),
                    epoch: self.epoch,
                    received_ms: self.displayed.received_ms,
                    received: current.received,
                    live,
                    skipped,
                };
                ui.painter()
                    .add(egui_wgpu::Callback::new_paint_callback(rect, scene));
                let label = if self.config.lock().unwrap().mapping == Mapping::Channels {
                    "X AF7  ·  Y AF8  ·  Z TP9  ·  COLOR TP10"
                } else {
                    "X V(t)  ·  Y V(t+τ)  ·  Z V(t+2τ)"
                };
                ui.painter().text(
                    rect.left_top() + egui::vec2(10.0, 18.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::monospace(11.0),
                    egui::Color32::from_gray(140),
                );
                ui.painter().text(
                    rect.left_top() + egui::vec2(10.0, 38.0),
                    egui::Align2::LEFT_TOP,
                    "Geometry describes signal relationships, not focus or intent.",
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_gray(110),
                );
                if self.semantics {
                    self.semantic_scene(ui, rect);
                }
                let health = self
                    .displayed
                    .rms
                    .iter()
                    .zip(["TP9", "AF7", "AF8", "TP10"])
                    .map(|(v, n)| {
                        format!(
                            "{n} RMS {}",
                            v.map(|x| format!("{x:.1} µV"))
                                .unwrap_or("unavailable".into())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("  ·  ");
                ui.painter().text(
                    rect.right_top() + egui::vec2(-10.0, 18.0),
                    egui::Align2::RIGHT_TOP,
                    health,
                    egui::FontId::monospace(10.0),
                    egui::Color32::from_gray(135),
                );
                ui.painter().text(
                    rect.right_top() + egui::vec2(-10.0, 38.0),
                    egui::Align2::RIGHT_TOP,
                    format!(
                        "Out of range: {}/{} · drag to orbit · scroll to zoom",
                        self.displayed.geometry.clipped, self.displayed.geometry.examined
                    ),
                    egui::FontId::monospace(10.0),
                    egui::Color32::from_gray(115),
                );
            });
        if !self.context.is_empty() {
            egui::Window::new("Historical context · text and scores only")
                .default_open(false)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for turn in &self.context {
                            ui.label(format!("Turn {} · {}", turn.index, turn.heard));
                            for c in &turn.candidates {
                                ui.label(format!(
                                    "{} · potential {:.4} · {}",
                                    c.role_id, c.potential, c.text
                                ));
                            }
                            ui.separator();
                        }
                    });
                });
        }
        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / 60.0));
    }
    fn on_exit(&mut self) {
        self.finish();
    }
}
fn main() {
    let args = Args::parse().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("NeuralCompose · phase space"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "NeuralCompose phase space",
        options,
        Box::new(move |cc| {
            App::new(cc, args)
                .map(|a| Box::new(a) as Box<dyn eframe::App>)
                .map_err(|e| e.into())
        }),
    ) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
