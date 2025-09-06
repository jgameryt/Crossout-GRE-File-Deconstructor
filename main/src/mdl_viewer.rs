use egui::{self, ColorImage, ComboBox, Response, TextureHandle, Vec2};
use std::collections::BTreeSet;
use crate::mdl::{MdlChunk, group_models, ModelGroup};

pub struct ModelViewer {
    // Data
    pub chunks: Vec<MdlChunk>,
    pub groups: Vec<ModelGroup>,
    pub selected_group: usize,
    pub selected_lod: usize,

    // Render state
    tex: Option<TextureHandle>,
    buf: ColorImage,
    yaw: f32,
    pitch: f32,
    dist: f32,
}

impl ModelViewer {
    pub fn new(chunks: Vec<MdlChunk>) -> Self {
        let groups = group_models(&chunks);
        Self {
            chunks,
            groups,
            selected_group: 0,
            selected_lod: 0,
            tex: None,
            buf: ColorImage::filled([512, 512], egui::Color32::BLACK),
            yaw: 0.5,
            pitch: 0.2,
            dist: 3.0,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(512.0);
        ui.set_max_width(512.0);

        ComboBox::from_label("Model")
            .selected_text(format!("Model {}", self.selected_group))
            .show_ui(ui, |ui| {
                for (idx, _g) in self.groups.iter().enumerate() {
                    if ui
                        .selectable_value(&mut self.selected_group, idx, format!("Model {}", idx))
                        .clicked()
                    {
                        self.selected_lod = 0;
                        ui.ctx().request_repaint();
                    }
                }
            });
        // Clamp selected_lod to existing range
        let g = &self.groups[self.selected_group];
        if self.selected_lod >= g.lods.len() {
            self.selected_lod = 0;
        }
        ComboBox::from_label("LOD")
            .selected_text(format!("LOD {}", self.selected_lod))
            .show_ui(ui, |ui| {
                for (lod_rank, _) in g.lods.iter().enumerate() {
                    if ui
                        .selectable_value(&mut self.selected_lod, lod_rank, format!("LOD {}", lod_rank))
                        .clicked()
                    {
                        ui.ctx().request_repaint();
                    }
                }
            });

        let size = Vec2::splat(512.0);
        self.render_current();
        let tex = self
            .tex
            .get_or_insert_with(|| ui.ctx().load_texture("mdl_view", self.buf.clone(), egui::TextureOptions::LINEAR));
        tex.set(self.buf.clone(), egui::TextureOptions::LINEAR);
        let resp = ui
            .add(egui::Image::new((tex.id(), size)).sense(egui::Sense::drag()))
            .on_hover_cursor(egui::CursorIcon::Grab);
        self.handle_input(ui, &resp);
    }

    fn handle_input(&mut self, ui: &egui::Ui, resp: &Response) {
        if resp.dragged() {
            let d = resp.drag_delta();
            self.yaw -= d.x * 0.005;
            self.pitch = (self.pitch - d.y * 0.005).clamp(-1.2, 1.2);
            ui.ctx().request_repaint();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                let dz = scroll * -0.002;
                self.dist = (self.dist * (1.0 + dz)).clamp(0.5, 20.0);
                ui.ctx().request_repaint();
            }
        }
    }

    fn render_current(&mut self) {
        let ch = {
            let g = &self.groups[self.selected_group];
            &self.chunks[g.lods[self.selected_lod]]
        };
        let w = self.buf.size[0] as i32;
        let h = self.buf.size[1] as i32;
        self.buf.pixels.fill(egui::Color32::from_rgb(15, 15, 20));
        let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
        for tri in &ch.indices {
            let (a, b, c) = (tri[0], tri[1], tri[2]);
            let e = |x: u32, y: u32| if x < y { (x, y) } else { (y, x) };
            edges.insert(e(a, b));
            edges.insert(e(b, c));
            edges.insert(e(c, a));
        }
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let mut screen: Vec<[i32; 2]> = Vec::with_capacity(ch.vertices.len());
        for &p in &ch.vertices {
            let mut x = p[0] * cy + p[2] * sy;
            let mut z = -p[0] * sy + p[2] * cy;
            let mut y = p[1] * cp - z * sp;
            z = p[1] * sp + z * cp;
            let zc = z + self.dist * 2.0 + 1.0;
            let f = 300.0 / zc.max(0.01);
            let sx = (w / 2) as f32 + x * f;
            let sy2 = (h / 2) as f32 - y * f;
            screen.push([sx as i32, sy2 as i32]);
        }
        for (a, b) in edges {
            let pa = screen[a as usize];
            let pb = screen[b as usize];
            self.line(pa[0], pa[1], pb[0], pb[1], egui::Color32::WHITE);
        }
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: egui::Color32) {
        let w = self.buf.size[0] as i32;
        let h = self.buf.size[1] as i32;
        let mut x0 = x0;
        let mut y0 = y0;
        let mut x1 = x1;
        let mut y1 = y1;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x0 >= 0 && x0 < w && y0 >= 0 && y0 < h {
                self.buf[(x0 as usize, y0 as usize)] = color;
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
}

