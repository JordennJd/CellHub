use cellhub_simulation::{SIZE, Simulation};
use eframe::egui::{self, Color32, Rect, Sense, Stroke, Vec2};
use wasm_bindgen::prelude::*;

const NAMES: [&str; 6] = [
    "Фотоцит",
    "Фагоцит",
    "Жгутикоцит",
    "Вироцит",
    "Кератиноцит",
    "Девороцит",
];
const COLORS: [Color32; 6] = [
    Color32::from_rgb(164, 219, 113),
    Color32::from_rgb(241, 191, 114),
    Color32::from_rgb(113, 203, 212),
    Color32::from_rgb(201, 151, 237),
    Color32::from_rgb(179, 195, 208),
    Color32::from_rgb(240, 131, 115),
];
fn genes() -> Vec<f32> {
    (0..8)
        .flat_map(|i| [(i % 6) as f32, 2.4, i as f32, i as f32, 0.5, 12., 0.8, 240.])
        .collect()
}

struct Lab {
    sim: Simulation,
    genes: Vec<f32>,
    mode: usize,
    speed: f32,
    debt: f32,
    cells: Vec<f32>,
    selected: Option<u32>,
    tool: usize,
    layer: usize,
    center: Vec2,
    zoom: f32,
    message: String,
}
impl Default for Lab {
    fn default() -> Self {
        Self {
            sim: Simulation::new(73421),
            genes: genes(),
            mode: 0,
            speed: 1.,
            debt: 0.,
            cells: vec![],
            selected: None,
            tool: 0,
            layer: 0,
            center: Vec2::splat(SIZE / 2.),
            zoom: 0.,
            message: String::new(),
        }
    }
}
impl Lab {
    fn populate(&mut self, n: usize) {
        self.sim.reset();
        self.selected = None;
        let g = genes();
        for i in 0..n {
            let x = 20. + (i % 71) as f32 * 42.;
            let y = 20. + (i / 71) as f32 * 42.;
            if let Err(e) = self.sim.spawn(x, y, i % 6, &g) {
                self.message = format!("{e:?}");
                break;
            }
        }
    }
    fn result(&mut self, r: Result<(), JsError>) {
        if let Err(e) = r {
            self.message = format!("{e:?}");
        }
    }
}
impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.debt += ctx.input(|i| i.stable_dt).min(0.25) * self.speed;
        let steps = (self.debt * 30.).floor() as u32;
        if steps > 0 {
            self.sim.advance(steps);
            self.debt = (self.debt - steps as f32 / 30.).max(0.);
        }
        self.cells = self.sim.snapshot();
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("CellHub");
                for (s, label) in [(0., "Пауза"), (1., "1×"), (2., "2×"), (4., "4×")] {
                    ui.selectable_value(&mut self.speed, s, label);
                }
                if ui.button("Шаг").clicked() && self.speed == 0. {
                    self.sim.advance(1);
                }
                if ui.button("Очистить").clicked() {
                    self.sim.reset();
                    self.selected = None;
                }
                if ui.button("Вся карта").clicked() {
                    self.zoom = 0.;
                    self.center = Vec2::splat(SIZE / 2.);
                }
                ui.label(format!("Клеток: {} / 5000", self.cells.len() / 10));
            });
        });
        egui::SidePanel::right("editor")
            .default_width(290.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Редактор генома");
                    ui.horizontal_wrapped(|ui| {
                        for i in 0..8 {
                            ui.selectable_value(&mut self.mode, i, format!("M{}", i + 1));
                        }
                    });
                    let offset = self.mode * 8;
                    egui::ComboBox::from_label("Тип")
                        .selected_text(NAMES[self.genes[offset] as usize])
                        .show_ui(ui, |ui| {
                            for (i, name) in NAMES.iter().enumerate() {
                                ui.selectable_value(&mut self.genes[offset], i as f32, *name);
                            }
                        });
                    for (k, label, min, max) in [
                        (1, "Масса деления", 1., 8.),
                        (2, "Режим ребёнка A (0–7)", 0., 7.),
                        (3, "Режим ребёнка B (0–7)", 0., 7.),
                        (4, "Доля массы A", 0.2, 0.8),
                        (5, "Скорость", 0., 60.),
                        (6, "Угол (рад)", -3.14, 3.14),
                        (7, "Жизнь (с)", 20., 1000.),
                    ] {
                        let mut slider =
                            egui::Slider::new(&mut self.genes[offset + k], min..=max).text(label);
                        if k == 2 || k == 3 {
                            slider = slider.integer();
                        }
                        ui.add(slider);
                    }
                    if let Some(id) = self.selected {
                        ui.separator();
                        ui.label(format!("Клетка #{id}"));
                        if let Some(c) = self.cells.chunks_exact(10).find(|c| c[0] as u32 == id) {
                            ui.label(format!(
                                "Масса {:.3}, возраст {:.1}, поколение {}, мутации {}",
                                c[5], c[6], c[7], c[8]
                            ));
                            if ui.button("Применить геном").clicked() {
                                let r = self.sim.edit(id, self.mode, &self.genes);
                                self.result(r);
                            }
                        } else {
                            ui.label("Клетка разделилась или погибла");
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Сохранить").clicked() {
                            let r = (|| {
                                let s = web_sys::window()
                                    .unwrap()
                                    .local_storage()?
                                    .ok_or(JsValue::from_str("Storage unavailable"))?;
                                s.set_item(
                                    "cellhub.rust.genome.v1",
                                    &serde_json::to_string(&self.genes).unwrap(),
                                )
                            })();
                            self.message = match r {
                                Ok(()) => "Геном сохранён".into(),
                                Err(e) => format!("{e:?}"),
                            };
                        }
                        if ui.button("Загрузить").clicked() {
                            let loaded = web_sys::window()
                                .and_then(|w| w.local_storage().ok().flatten())
                                .and_then(|s| s.get_item("cellhub.rust.genome.v1").ok().flatten())
                                .and_then(|s| serde_json::from_str::<Vec<f32>>(&s).ok());
                            if let Some(g) = loaded.filter(|g| {
                                g.len() == 64
                                    && g.chunks_exact(8).all(|m| {
                                        m.iter().all(|v| v.is_finite())
                                            && m[0] >= 0.
                                            && m[0] <= 5.
                                            && m[0].fract() == 0.
                                            && m[2] >= 0.
                                            && m[2] <= 7.
                                            && m[3] >= 0.
                                            && m[3] <= 7.
                                    })
                            }) {
                                self.genes = g;
                            } else {
                                self.message = "Нет совместимого генома".into();
                            }
                        }
                    });
                    if ui.button("Стандартный геном").clicked() {
                        self.genes = genes();
                    }
                    ui.separator();
                    ui.label("Заменить популяцию:");
                    for n in [60, 1000, 5000] {
                        if ui.button(format!("{n} клеток")).clicked() {
                            self.populate(n);
                        }
                    }
                    let stats = self.sim.stats();
                    ui.label(format!(
                        "Время {:.1} с · рождений {} · смертей {} · заражений {}",
                        stats[0] / 30.,
                        stats[2],
                        stats[3],
                        stats[4]
                    ));
                    ui.label(&self.message);
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, name) in ["Поместить", "Выбрать", "Камера"].iter().enumerate()
                {
                    ui.selectable_value(&mut self.tool, i, *name);
                }
                for (i, name) in ["Биомы", "Свет", "Радиация", "Пища"].iter().enumerate()
                {
                    ui.selectable_value(&mut self.layer, i, *name);
                }
            });
            let (response, p) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = response.rect;
            if self.zoom == 0. {
                self.zoom = rect.width().min(rect.height()) / SIZE * 0.93;
            }
            if response.dragged() {
                self.center -= response.drag_delta() / self.zoom;
            }
            if response.hovered() {
                let delta = ctx.input(|i| i.smooth_scroll_delta.y);
                if delta != 0. {
                    if let Some(pos) = response.hover_pos() {
                        let before = (pos - rect.center()) / self.zoom + self.center;
                        self.zoom = (self.zoom * (delta * 0.001).exp()).clamp(0.05, 4.);
                        self.center = before - (pos - rect.center()) / self.zoom;
                    }
                }
            }
            let project =
                |x: f32, y: f32| rect.center() + (Vec2::new(x, y) - self.center) * self.zoom;
            let terrain = self.sim.terrain();
            for (i, t) in terrain.chunks_exact(4).enumerate() {
                let r = Rect::from_min_size(
                    project((i % 48) as f32 * 64., (i / 48) as f32 * 64.),
                    Vec2::splat(64. * self.zoom),
                );
                if !r.intersects(rect) {
                    continue;
                }
                let color = if self.layer == 0 {
                    [
                        Color32::from_rgb(37, 62, 43),
                        Color32::from_rgb(35, 60, 56),
                        Color32::from_rgb(21, 45, 57),
                        Color32::from_rgb(60, 41, 64),
                    ][t[3] as usize]
                } else {
                    let v = t[self.layer - 1];
                    Color32::from_rgb(
                        (40. + v * 100.) as u8,
                        (40. + v * 140.) as u8,
                        (45. + v * 80.) as u8,
                    )
                };
                p.rect_filled(r, 0., color);
            }
            for c in self.cells.chunks_exact(10) {
                let pos = project(c[1], c[2]);
                let r = c[3] * self.zoom;
                if !rect.expand(r + 8.).contains(pos) {
                    continue;
                }
                let color = COLORS[c[4] as usize];
                p.circle_filled(pos, r, color.gamma_multiply(0.3));
                p.circle_stroke(pos, r, Stroke::new(1_f32, color));
                p.circle_filled(pos + Vec2::angled(c[9]) * r * 0.2, r * 0.29, color);
                if c[4] == 2. {
                    p.line_segment(
                        [
                            pos - Vec2::angled(c[9]) * r,
                            pos - Vec2::angled(c[9]) * r * 2.,
                        ],
                        Stroke::new(1_f32, color),
                    );
                }
                if c[8] > 0. {
                    p.circle_filled(pos + Vec2::new(r * 0.55, -r * 0.55), 2., Color32::WHITE);
                }
                if self.selected == Some(c[0] as u32) {
                    p.circle_stroke(pos, r + 5., Stroke::new(1_f32, Color32::WHITE));
                }
            }
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let w = (pos - rect.center()) / self.zoom + self.center;
                    if w.x >= 0. && w.y >= 0. && w.x <= SIZE && w.y <= SIZE {
                        if self.tool == 0 {
                            let r = self.sim.spawn(w.x, w.y, self.mode, &self.genes).map(|_| ());
                            self.result(r);
                        } else if self.tool == 1 {
                            self.selected = self
                                .cells
                                .chunks_exact(10)
                                .filter(|c| {
                                    (Vec2::new(c[1], c[2]) - w).length() < c[3].max(12. / self.zoom)
                                })
                                .min_by(|a, b| {
                                    (Vec2::new(a[1], a[2]) - w)
                                        .length_sq()
                                        .total_cmp(&(Vec2::new(b[1], b[2]) - w).length_sq())
                                })
                                .map(|c| c[0] as u32);
                            if let Some(id) = self.selected {
                                self.genes = self.sim.genome(id);
                                self.mode = self.sim.cell_mode(id);
                            }
                        }
                    }
                }
            }
        });
        ctx.request_repaint();
    }
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn start(
    canvas: web_sys::HtmlCanvasElement,
    count: usize,
    paused: bool,
) -> Result<(), JsValue> {
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |_| {
                let mut lab = Lab::default();
                if count > 0 {
                    lab.populate(count.min(5000));
                }
                if paused {
                    lab.speed = 0.;
                }
                Ok(Box::new(lab))
            }),
        )
        .await
}
