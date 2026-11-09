//! Rust owns only canvas rendering; React owns UI and the Worker owns simulation.
#![cfg(target_arch = "wasm32")]
use eframe::egui::{self, Align2, Color32, FontId, Rect, Stroke, Vec2};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;
const WORLD: f32 = 3072.;
const COLORS: [u32; 6] = [0xa4db71, 0xf1bf72, 0x71cbd4, 0xc997ed, 0xb3c3d0, 0xf08373];
fn rgb(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
fn alpha(v: u32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
        (a.clamp(0., 1.) * 255.).round() as u8,
    )
}
struct Scene {
    cells: Vec<f32>,
    terrain: Vec<f32>,
    x: f32,
    y: f32,
    zoom: f32,
    layer: u32,
    selected: i32,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            cells: vec![],
            terrain: vec![],
            x: WORLD / 2.,
            y: WORLD / 2.,
            zoom: 1.,
            layer: 0,
            selected: -1,
        }
    }
}
struct CanvasApp(Rc<RefCell<Scene>>);
impl eframe::App for CanvasApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let s = self.0.borrow();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(rgb(0x091411)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let p = ui.painter();
                let z = s.zoom;
                let project =
                    |x: f32, y: f32| rect.center() + Vec2::new((x - s.x) * z, (y - s.y) * z);
                for (i, t) in s.terrain.chunks_exact(4).take(48 * 48).enumerate() {
                    let x = (i % 48) as f32 * 64.;
                    let y = (i / 48) as f32 * 64.;
                    let tile = Rect::from_min_size(project(x, y), Vec2::splat(64. * z));
                    if !tile.intersects(rect) {
                        continue;
                    }
                    let color = match s.layer {
                        1 => alpha(0xe7cf7d, 0.08 + t[0] * 0.72),
                        2 => alpha(0xd28ce9, 0.08 + t[1] * 0.72),
                        3 => alpha(0x75d5bd, 0.08 + t[2] * 0.72),
                        _ => rgb([0x253e2b, 0x233c38, 0x152d39, 0x3c2940][(t[3] as usize).min(3)]),
                    };
                    p.rect_filled(tile, 0., color);
                    if t[2] > 0.15 {
                        p.circle_filled(
                            project(
                                x + 17. + ((i / 48) % 3) as f32 * 9.,
                                y + 27. + (i % 48 % 3) as f32 * 7.,
                            ),
                            1.8 * z,
                            alpha(0xa5cf9b, t[2] * 0.5),
                        );
                    }
                }
                let corners = [
                    project(0., 0.),
                    project(WORLD, 0.),
                    project(WORLD, WORLD),
                    project(0., WORLD),
                ];
                for i in 0..4 {
                    p.line_segment(
                        [corners[i], corners[(i + 1) % 4]],
                        Stroke::new(2. * z, rgb(0x7b9889)),
                    );
                }
                for (name, x, y) in [
                    ("СОЛНЕЧНОЕ МЕЛКОВОДЬЕ", 470., 650.),
                    ("ПИТАТЕЛЬНЫЙ ПОЯС", 1750., 1550.),
                    ("ТЁМНЫЕ ГЛУБИНЫ", 1250., 2600.),
                    ("РАДИАЦИОННЫЙ КАРМАН", 2330., 680.),
                ] {
                    let spaced = name
                        .chars()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join("\u{2009}");
                    p.text(
                        project(x, y),
                        Align2::LEFT_TOP,
                        spaced,
                        FontId::monospace(20. * z),
                        alpha(0xa4bbae, 0.5),
                    );
                }
                for c in s.cells.chunks_exact(10) {
                    let pos = project(c[1], c[2]);
                    let r = c[3] * z;
                    if !rect.expand((c[3] * 2. + 7.) * z).contains(pos) {
                        continue;
                    }
                    let color = COLORS.get(c[4] as usize).copied().unwrap_or(0xffffff);
                    p.circle_filled(pos, r + 3. * z, alpha(color, 0.08));
                    p.circle_filled(pos, r, alpha(color, 0.22));
                    p.circle_stroke(pos, r, Stroke::new(1.4 * z, rgb(color)));
                    p.circle_filled(
                        pos + Vec2::angled(c[9]) * r * 0.2,
                        r * 0.29,
                        alpha(color, 0.9),
                    );
                    if c[4] == 2. {
                        p.line_segment(
                            [
                                pos - Vec2::angled(c[9]) * r,
                                pos - Vec2::angled(c[9]) * r * 2.,
                            ],
                            Stroke::new(z, rgb(color)),
                        );
                    }
                    if c[8] > 0. {
                        p.circle_filled(
                            pos + Vec2::new(r * 0.55, -r * 0.55),
                            2. * z,
                            Color32::WHITE,
                        );
                    }
                    if c[0] as i32 == s.selected {
                        p.circle_stroke(pos, r + 7. * z, Stroke::new(1.5 * z, Color32::WHITE));
                    }
                }
            });
    }
}
#[wasm_bindgen]
pub struct WorldRenderer {
    scene: Rc<RefCell<Scene>>,
    runner: eframe::WebRunner,
    ctx: egui::Context,
}
#[wasm_bindgen]
impl WorldRenderer {
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<WorldRenderer, JsValue> {
        let scene = Rc::new(RefCell::new(Scene::default()));
        let runner = eframe::WebRunner::new();
        let context = Rc::new(RefCell::new(None));
        let output = context.clone();
        let shared = scene.clone();
        let options = eframe::WebOptions {
            should_propagate_event: Box::new(|_| true),
            ..Default::default()
        };
        runner
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    *output.borrow_mut() = Some(cc.egui_ctx.clone());
                    Ok(Box::new(CanvasApp(shared)))
                }),
            )
            .await?;
        let ctx = context
            .borrow_mut()
            .take()
            .ok_or_else(|| JsValue::from_str("Renderer context unavailable"))?;
        Ok(Self { scene, runner, ctx })
    }
    pub fn set_frame(&self, cells: &[f32], terrain: &[f32]) -> Result<(), JsValue> {
        if cells.len() % 10 != 0
            || terrain.len() != 48 * 48 * 4
            || cells.iter().chain(terrain).any(|v| !v.is_finite())
        {
            return Err(JsValue::from_str("Invalid render snapshot"));
        }
        let mut s = self.scene.borrow_mut();
        s.cells.clear();
        s.cells.extend_from_slice(cells);
        s.terrain.clear();
        s.terrain.extend_from_slice(terrain);
        self.ctx.request_repaint();
        Ok(())
    }
    pub fn set_view(&self, x: f32, y: f32, zoom: f32, layer: u32, selected: i32) {
        let mut s = self.scene.borrow_mut();
        if x.is_finite() && y.is_finite() && zoom.is_finite() && zoom > 0. {
            s.x = x;
            s.y = y;
            s.zoom = zoom;
            s.layer = layer;
            s.selected = selected;
        }
        self.ctx.request_repaint();
    }
    pub fn destroy(&self) {
        self.runner.destroy();
    }
}
