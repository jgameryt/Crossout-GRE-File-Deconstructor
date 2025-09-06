use egui::{ColorImage, TextureHandle, Vec2};
use crate::tfd::TfdImage;

pub struct TextureViewer {
    image: ColorImage,
    tex: Option<TextureHandle>,
}

impl TextureViewer {
    pub fn new(img: TfdImage) -> Self {
        let image = ColorImage::from_rgba_unmultiplied(
            [img.width, img.height],
            &img.rgba,
        );
        Self { image, tex: None }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let tex = self.tex.get_or_insert_with(|| {
            ui.ctx().load_texture("tfd_view", self.image.clone(), egui::TextureOptions::LINEAR)
        });
        tex.set(self.image.clone(), egui::TextureOptions::LINEAR);
        let size = Vec2::new(self.image.size[0] as f32, self.image.size[1] as f32);
        ui.image((tex.id(), size));
    }
}
