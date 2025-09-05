use egui::{self, ColorImage, TextureHandle, Vec2};
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
            chunks, groups,
            selected_group: 0,
            selected_lod: 0,
            tex: None,
            buf: ColorImage::new([512,512], vec![egui::Color32::BLACK; 512*512]),
            yaw: 0.5, pitch: 0.2, dist: 3.0,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Left: selector
            ui.vertical(|ui| {
                ui.heading("Meshes");
                for (gidx, g) in self.groups.iter().enumerate() {
                    let label = format!("Model {} ({} LODs)", gidx, g.lods.len());
                    egui::CollapsingHeader::new(label).default_open(gidx==self.selected_group).show(ui, |ui| {
                        for (lod_rank, &chunk_idx) in g.lods.iter().enumerate() {
                            let ch = &self.chunks[chunk_idx];
                            let selected = self.selected_group==gidx && self.selected_lod==lod_rank;
                            let lbl = format!("LOD{} – {}v / {}f", lod_rank, ch.vcount, ch.icount/3);
                            if ui.selectable_label(selected, lbl).clicked() {
                                self.selected_group=gidx; self.selected_lod=lod_rank;
                            }
                        }
                    });
                }
                ui.separator();
                ui.label("Drag to rotate, scroll to zoom");
            });

            ui.separator();

            // Right: viewer
            ui.vertical(|ui| {
                let size = ui.available_size().min(Vec2::splat(512.0));
                self.handle_input(ui);
                self.render_current();
                let tex = self.tex.get_or_insert_with(|| ui.ctx().load_texture("mdl_view", self.buf.clone(), egui::TextureOptions::LINEAR));
                tex.set(self.buf.clone(), egui::TextureOptions::LINEAR);
                ui.image((tex.id(), size));
            });
        });
    }

    fn handle_input(&mut self, ui: &mut egui::Ui) {
        let resp = ui.interact(ui.max_rect(), ui.id().with("viewer_drag"), egui::Sense::drag());
        if resp.dragged() {
            let d = resp.drag_delta();
            self.yaw += d.x * 0.005;
            self.pitch = (self.pitch + d.y * 0.005).clamp(-1.2, 1.2);
            ui.ctx().request_repaint();
        }
        // Scroll wheel zoom
        if ui.input(|i| i.raw_scroll_delta.y != 0.0) {
            let dz = ui.input(|i| i.raw_scroll_delta.y) * -0.002;
            self.dist = (self.dist * (1.0 + dz)).clamp(0.5, 20.0);
            ui.ctx().request_repaint();
        }
    }

    fn render_current(&mut self) {
        let ch = {
            let g = &self.groups[self.selected_group];
            &self.chunks[g.lods[self.selected_lod]]
        };
        // Simple CPU wireframe raster
        let w = self.buf.size[0] as i32;
        let h = self.buf.size[1] as i32;
        self.buf.pixels.fill(egui::Color32::from_rgb(15,15,20));
        // Build simple edge set
        let mut edges: BTreeSet<(u32,u32)> = BTreeSet::new();
        for tri in &ch.indices {
            let (a,b,c) = (tri[0], tri[1], tri[2]);
            let e = |x:u32,y:u32| if x<y {(x,y)} else {(y,x)};
            edges.insert(e(a,b)); edges.insert(e(b,c)); edges.insert(e(c,a));
        }
        // Transform + project
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let mut screen: Vec<[i32;2]> = Vec::with_capacity(ch.vertices.len());
        let mut zbuf: Vec<f32> = Vec::with_capacity(ch.vertices.len());
        for &p in &ch.vertices {
            // rotate yaw around Y, then pitch around X
            let mut x =  p[0]*cy + p[2]*sy;
            let mut z = -p[0]*sy + p[2]*cy;
            let mut y =  p[1]*cp - z*sp;
            z =  p[1]*sp + z*cp;
            // camera
            let zc = z + self.dist*2.0 + 1.0;
            let f = 300.0 / zc.max(0.01);
            let sx = (w/2)  as f32 + x*f;
            let sy2 = (h/2) as f32 - y*f;
            screen.push([sx as i32, sy2 as i32]);
            zbuf.push(zc);
        }
        // Draw edges
        for (a,b) in edges {
            let pa = screen[a as usize];
            let pb = screen[b as usize];
            self.line(pa[0], pa[1], pb[0], pb[1], egui::Color32::WHITE);
        }
    }

    fn line(&mut self, x0:i32, y0:i32, x1:i32, y1:i32, color: egui::Color32) {
        let w = self.buf.size[0] as i32;
        let h = self.buf.size[1] as i32;
        let mut x0=x0; let mut y0=y0; let mut x1=x1; let mut y1=y1;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x0>=0 && x0<w && y0>=0 && y0<h {
                self.buf[(x0 as usize, y0 as usize)] = color;
            }
            if x0 == x1 && y0 == y1 { break; }
            let e2 = 2*err;
            if e2 >= dy { err += dy; x0 += sx; }
            if e2 <= dx { err += dx; y0 += sy; }
        }
    }
}

