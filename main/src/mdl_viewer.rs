use egui::{self, ColorImage, ComboBox, TextureHandle, Vec2};
use sokol::{gfx as sg, gl as sgl};
use crate::mdl::{MdlChunk, group_models, ModelGroup};

pub struct ModelViewer {
    // Data
    pub chunks: Vec<MdlChunk>,
    pub groups: Vec<ModelGroup>,
    pub selected_group: usize,
    pub selected_lod: usize,

    // Render state
    tex: Option<TextureHandle>,
    pixels: Vec<u8>,
    yaw: f32,
    pitch: f32,
    dist: f32,
    image: sg::Image,
    pass: sg::Pass,
    pass_action: sg::PassAction,
}

impl ModelViewer {
    pub fn new(chunks: Vec<MdlChunk>) -> Self {
        let groups = group_models(&chunks);
        sg::setup(&sg::Desc::default());
        sgl::setup(&sgl::Desc::default());
        let color_img = sg::make_image(&sg::ImageDesc {
            render_target: true,
            width: 512,
            height: 512,
            pixel_format: sg::PixelFormat::RGBA8,
            ..Default::default()
        });
        let depth_img = sg::make_image(&sg::ImageDesc {
            render_target: true,
            width: 512,
            height: 512,
            pixel_format: sg::PixelFormat::DEPTH,
            ..Default::default()
        });
        let pass = sg::make_pass(&sg::PassDesc {
            color_attachments: [
                sg::PassAttachment { image: color_img, ..Default::default() },
                sg::PassAttachment::default(),
                sg::PassAttachment::default(),
                sg::PassAttachment::default(),
            ],
            depth_stencil_attachment: sg::PassAttachment { image: depth_img, ..Default::default() },
            ..Default::default()
        });
        Self {
            chunks,
            groups,
            selected_group: 0,
            selected_lod: 0,
            tex: None,
            pixels: vec![0; 512 * 512 * 4],
            yaw: 0.5,
            pitch: 0.2,
            dist: 3.0,
            image: color_img,
            pass,
            pass_action: sg::PassAction::default(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(512.0);
        ui.set_max_width(512.0);

        ComboBox::from_label("Model")
            .selected_text(format!("Model {}", self.selected_group))
            .show_ui(ui, |ui| {
                for (idx, _g) in self.groups.iter().enumerate() {
                    ui.selectable_value(&mut self.selected_group, idx, format!("Model {}", idx));
                }
            });
        let g = &self.groups[self.selected_group];
        ComboBox::from_label("LOD")
            .selected_text(format!("LOD {}", self.selected_lod))
            .show_ui(ui, |ui| {
                for (lod_rank, _) in g.lods.iter().enumerate() {
                    ui.selectable_value(&mut self.selected_lod, lod_rank, format!("LOD {}", lod_rank));
                }
            });

        let size = Vec2::splat(512.0);
        self.handle_input(ui);
        self.render_current();
        let img = ColorImage::from_rgba_unmultiplied([512, 512], &self.pixels);
        let tex = self
            .tex
            .get_or_insert_with(|| ui.ctx().load_texture("mdl_view", img.clone(), egui::TextureOptions::LINEAR));
        tex.set(img, egui::TextureOptions::LINEAR);
        ui.image((tex.id(), size));
    }

    fn handle_input(&mut self, ui: &mut egui::Ui) {
        let resp = ui.interact(ui.max_rect(), ui.id().with("viewer_drag"), egui::Sense::drag());
        if resp.dragged() {
            let d = resp.drag_delta();
            self.yaw += d.x * 0.005;
            self.pitch = (self.pitch + d.y * 0.005).clamp(-1.2, 1.2);
            ui.ctx().request_repaint();
        }
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
        sg::begin_pass(self.pass, &self.pass_action);
        sg::apply_viewport(0, 0, 512, 512, true);
        sgl::defaults();
        sgl::matrix_mode_projection();
        sgl::load_identity();
        sgl::perspective(60.0, 1.0, 0.01, 100.0);
        sgl::matrix_mode_modelview();
        sgl::load_identity();
        sgl::translate(0.0, 0.0, -self.dist * 2.0 - 1.0);
        sgl::rotate(self.pitch, 1.0, 0.0, 0.0);
        sgl::rotate(self.yaw, 0.0, 1.0, 0.0);
        sgl::begin_triangles();
        for tri in &ch.indices {
            for &idx in tri {
                let v = ch.vertices[idx as usize];
                sgl::v3f(v[0], v[1], v[2]);
            }
        }
        sgl::end();
        sgl::draw();
        sg::end_pass();
        sg::commit();
        sg::read_pixels(&self.image, &mut self.pixels);
    }
}

