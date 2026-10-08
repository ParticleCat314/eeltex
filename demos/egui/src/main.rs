//! Benchmark demo: formula updates per second within a per-frame time budget.
//! I'm not going to claim that this is a robust benchmark. Use it as an interesting indicator
//! of performance.
//!
//! ```text
//! cargo run --release                 # interactive: budget, size, speed cap, pause (space)
//! cargo run --release -- --bench 10   # run 10 seconds, print a summary, exit
//! ```

use std::time::{Duration, Instant};

use eeltex::export::atlas::{Atlas, Source};
use eeltex::{Fonts, Options, Scene};
use eframe::egui;

fn main() -> eframe::Result {
    let bench = std::env::args()
        .skip_while(|a| a != "--bench")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .or_else(|| std::env::args().any(|a| a == "--bench").then_some(10.0));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 750.0]),
        vsync: false,
        ..Default::default()
    };
    eframe::run_native(
        "eeltex egui bench",
        options,
        Box::new(move |_| Ok(Box::new(Bench::new(bench)))),
    )
}

struct Slot {
    scene: Scene,
    /// Layout scale of the scene; drawing divides coordinates by it.
    scale: f32,
}

#[derive(Default)]
struct Totals {
    updates: u64,
    frames: u64,
    render_time: Duration,
    worst: Duration,
    uploads: u64,
    errors: u64,
}

struct Bench {
    fonts: Fonts<'static>,
    atlas: Atlas,
    slots: Vec<Slot>,
    texture: Option<egui::TextureHandle>,
    counter: u64,
    budget_ms: f32,
    font_size: f32,
    /// Re-render at a different size each update, defeating the atlas's per-size cache.
    size_churn: bool,
    /// Bake glyphs at twice the device resolution and filter them down.
    sharp: bool,
    pixels_per_point: f32,
    paused: bool,
    /// Ceiling on updates per second (token bucket), or `None` for uncapped.
    speed_cap: Option<f32>,
    credit: f32,
    last_tick: Instant,
    totals: Totals,
    recent: Vec<(Instant, u32)>,
    started: Instant,
    bench_secs: Option<f32>,
}

const SLOTS: usize = 24;

impl Bench {
    fn new(bench_secs: Option<f32>) -> Bench {
        let mut bench = Bench {
            fonts: Fonts::embedded().expect("the embedded font parses"),
            atlas: Atlas::new(1024, 4096, Source::Coverage),
            slots: Vec::new(),
            texture: None,
            counter: 0,
            budget_ms: 6.0,
            font_size: 26.0,
            size_churn: false,
            sharp: false,
            pixels_per_point: 1.0,
            paused: false,
            speed_cap: None,
            credit: 0.0,
            last_tick: Instant::now(),
            totals: Totals::default(),
            recent: Vec::new(),
            started: Instant::now(),
            bench_secs,
        };
        for slot in 0..SLOTS {
            let scene = bench.render(slot, 0).expect("every template renders");
            bench.slots.push(Slot {
                scene,
                scale: bench.scale(),
            });
        }
        bench
    }

    /// Bitmap pixels per logical point of formula.
    fn scale(&self) -> f32 {
        self.pixels_per_point * if self.sharp { 2.0 } else { 1.0 }
    }

    fn render(&mut self, slot: usize, n: u64) -> Result<Scene, eeltex::Error> {
        let size = if self.size_churn {
            self.font_size + (n % 17) as f32
        } else {
            self.font_size
        };
        let scene = eeltex::render(
            &template(slot, n),
            &self.fonts,
            &Options::default().with_font_size(size * self.scale()),
        )?;
        self.atlas.prepare(&scene, &self.fonts);
        Ok(scene)
    }

    /// Top up the speed cap's token bucket, capped so a long pause cannot fund a burst.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        match self.speed_cap {
            Some(cap) => self.credit = (self.credit + cap * elapsed).min(cap * 0.25).max(0.0),
            // Uncapped: leave no stale credit for when the cap returns.
            None => self.credit = 0.0,
        }
    }

    fn churn(&mut self) -> u32 {
        self.refill();
        if self.paused {
            return 0;
        }
        let deadline = Instant::now() + Duration::from_secs_f32(self.budget_ms / 1000.0);
        let mut updates = 0u32;
        while Instant::now() < deadline {
            if self.speed_cap.is_some() {
                if self.credit < 1.0 {
                    break;
                }
                self.credit -= 1.0;
            }
            self.counter += 1;
            let slot = (self.counter as usize) % self.slots.len();
            let begin = Instant::now();
            let scale = self.scale();
            match self.render(slot, self.counter) {
                Ok(scene) => self.slots[slot] = Slot { scene, scale },
                Err(_) => self.totals.errors += 1,
            }
            let took = begin.elapsed();
            self.totals.render_time += took;
            self.totals.worst = self.totals.worst.max(took);
            self.totals.updates += 1;
            updates += 1;
        }
        updates
    }

    fn upload(&mut self, ctx: &egui::Context) {
        let resized = self.atlas.take_resized();
        let dirty = self.atlas.take_dirty();
        if !dirty && !resized && self.texture.is_some() {
            return;
        }
        // Premultiplied white at the coverage's alpha, so the vertex tint sets the ink colour.
        let pixels: Vec<egui::Color32> = self
            .atlas
            .data()
            .iter()
            .map(|&a| egui::Color32::from_white_alpha(a))
            .collect();
        let image = egui::ColorImage {
            size: [self.atlas.width() as usize, self.atlas.height() as usize],
            source_size: egui::Vec2::new(self.atlas.width() as f32, self.atlas.height() as f32),
            pixels,
        };
        self.totals.uploads += 1;
        // Nearest only when one texel is one device pixel; otherwise it is a resample.
        let filter = if (self.scale() - 1.0).abs() < f32::EPSILON {
            egui::TextureOptions::NEAREST
        } else {
            egui::TextureOptions::LINEAR
        };
        match (&mut self.texture, resized) {
            (Some(texture), false) => texture.set(image, filter),
            _ => {
                self.texture = Some(ctx.load_texture("eeltex-atlas", image, filter));
            }
        }
    }

    fn draw(&self, ui: &mut egui::Ui) {
        let Some(texture) = &self.texture else {
            return;
        };
        let ink = ui.visuals().text_color();
        let area = ui.available_rect_before_wrap();
        let columns = 4usize;
        let rows = SLOTS.div_ceil(columns);
        let cell = egui::vec2(area.width() / columns as f32, area.height() / rows as f32);

        let mut mesh = egui::Mesh::with_texture(texture.id());
        let painter = ui.painter();
        for (index, slot) in self.slots.iter().enumerate() {
            let scene = &slot.scene;
            let k = 1.0 / slot.scale;
            let corner = area.min
                + egui::vec2(
                    (index % columns) as f32 * cell.x,
                    (index / columns) as f32 * cell.y,
                );
            let origin = corner
                + egui::vec2(
                    (cell.x - scene.width * k).max(0.0) / 2.0,
                    (cell.y - scene.height() * k).max(0.0) / 2.0 + scene.ascent * k,
                );
            for rule in &scene.rules {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(origin.x + rule.x * k, origin.y + rule.y * k),
                        egui::vec2(rule.width * k, rule.height * k),
                    ),
                    0.0,
                    ink,
                );
            }
            for glyph in &scene.glyphs {
                let Some(quad) = self.atlas.quad(glyph) else {
                    continue;
                };
                mesh.add_rect_with_uv(
                    egui::Rect::from_min_max(
                        egui::pos2(origin.x + quad.min[0] * k, origin.y + quad.min[1] * k),
                        egui::pos2(origin.x + quad.max[0] * k, origin.y + quad.max[1] * k),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(quad.uv_min[0], quad.uv_min[1]),
                        egui::pos2(quad.uv_max[0], quad.uv_max[1]),
                    ),
                    ink,
                );
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }

    fn updates_per_second(&self) -> f32 {
        let cutoff = Instant::now() - Duration::from_secs(1);
        self.recent
            .iter()
            .filter(|(at, _)| *at > cutoff)
            .map(|(_, n)| *n)
            .sum::<u32>() as f32
    }

    fn summary(&self) -> String {
        let elapsed = self.started.elapsed().as_secs_f64();
        let per_update = if self.totals.updates > 0 {
            self.totals.render_time.as_secs_f64() * 1e6 / self.totals.updates as f64
        } else {
            0.0
        };
        format!(
            "updates: {} in {elapsed:.1}s ({:.0}/s)\n\
             render+atlas: mean {per_update:.1}us, worst {:.1}us\n\
             frames: {} ({:.0} fps)\n\
             texture uploads: {} ({}x{} atlas, {} entries)\n\
             errors: {}",
            self.totals.updates,
            self.totals.updates as f64 / elapsed,
            self.totals.worst.as_secs_f64() * 1e6,
            self.totals.frames,
            self.totals.frames as f64 / elapsed,
            self.totals.uploads,
            self.atlas.width(),
            self.atlas.height(),
            self.atlas.len(),
            self.totals.errors,
        )
    }
}

impl eframe::App for Bench {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pixels_per_point = ctx.pixels_per_point();
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.paused = !self.paused;
        }
        let updates = self.churn();
        let now = Instant::now();
        self.recent.push((now, updates));
        self.recent
            .retain(|(at, _)| now.duration_since(*at) < Duration::from_secs(2));
        self.totals.frames += 1;
        self.upload(ctx);

        egui::SidePanel::right("stats")
            .exact_width(260.0)
            .show(ctx, |ui| {
                ui.heading("eeltex churn");
                ui.label(format!("{:.0} updates/s", self.updates_per_second()));
                ui.label(format!("{updates} this frame"));
                ui.separator();
                let label = if self.paused { "resume" } else { "pause" };
                if ui.button(format!("{label} (space)")).clicked() {
                    self.paused = !self.paused;
                }
                let mut capped = self.speed_cap.is_some();
                if ui.checkbox(&mut capped, "cap speed").changed() {
                    self.speed_cap = capped.then_some(self.speed_cap.unwrap_or(500.0));
                }
                if let Some(cap) = &mut self.speed_cap {
                    ui.add(
                        egui::Slider::new(cap, 1.0..=20_000.0)
                            .logarithmic(true)
                            .text("max updates/s"),
                    );
                }
                ui.add(egui::Slider::new(&mut self.budget_ms, 0.5..=14.0).text("budget ms/frame"));
                ui.add(egui::Slider::new(&mut self.font_size, 14.0..=64.0).text("font px"));
                ui.checkbox(&mut self.size_churn, "size churn (defeat the atlas)");
                ui.checkbox(&mut self.sharp, "sharp glyphs (2x oversample)");
                ui.label(format!("bake scale {:.2}x", self.scale()));
                ui.separator();
                ui.label(self.summary());
            });
        egui::CentralPanel::default().show(ctx, |ui| self.draw(ui));

        if let Some(secs) = self.bench_secs
            && self.started.elapsed().as_secs_f32() >= secs
        {
            // Taken, not checked, so the summary prints once while the close command lands.
            self.bench_secs = None;
            println!("{}", self.summary());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // The benchmark is the frame loop; never wait for input.
        ctx.request_repaint();
    }
}

/// A formula for `slot` at update `n`, always valid, never twice the same.
fn template(slot: usize, n: u64) -> String {
    let a = n % 90 + 1;
    let b = n % 977 + 2;
    let c = n % 13 + 2;
    let d = n % 8971 + 3;
    match slot % 8 {
        0 => format!(r"\frac{{{a}}}{{{b}}} + \sqrt{{{d}}}"),
        1 => format!(r"\sum_{{n={a}}}^{{{b}}} \frac{{1}}{{n^{{{c}}}}}"),
        2 => format!(r"x = \frac{{-{a} \pm \sqrt{{{a}^2 - 4 \cdot {b}}}}}{{{c}}}"),
        3 => format!(r"\begin{{pmatrix}} {a} & {b} \\ {c} & {d} \end{{pmatrix}}"),
        4 => format!(r"\int_{{{a}}}^{{{b}}} e^{{-{c} t}} dt"),
        5 => format!(r"\left( \frac{{{a}}}{{{b} + \frac{{{c}}}{{{d}}}}} \right)^{{{c}}}"),
        6 => format!(r"\binom{{{b}}}{{{a}}} \xrightarrow{{{c}}} \hat{{x}}_{{{d}}}"),
        _ => format!(r"\lim_{{x \to {a}}} \frac{{x^{{{c}}} - {a}^{{{c}}}}}{{x - {a}}}"),
    }
}
