use core::num;
use std::{collections::{btree_map::Entry, BTreeMap}, fs, io::{Cursor, Read}, path::{Path, PathBuf}};
use anyhow::{Context, Result};
use eframe::egui::{self, Button};
use egui::debug_text::print;
use rfd::FileDialog;
mod mdl;
mod mdl_viewer;
use mdl_viewer::ModelViewer;
use egui::Align2;

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

#[derive(Debug, Clone)]
struct GrpEntry {
    index: u32,
    full_path: String,
    start:u64,
    size: u64,
    compression: Compression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression { Raw, Zstd } //I haven't found a single file that doesn't start with zstd magic yet

#[derive(Debug)]
struct GrpFile {
    path: PathBuf,
    file_data: Vec<u8>,
    header_size: u32,
    file_count: u32,
    data_start: u32,
    entries: Vec<GrpEntry>,
}

impl GrpFile {
    fn parse(grp_path: &Path) -> Result<Self> {
        let file_data = fs::read(grp_path).with_context(|| "select the grp file")?;
        // Check if true grp
        if &file_data[0..4] != b"GRP2" {
            anyhow::bail!("Not a GRP2 file");
        }
        let header_size = get_u32(&file_data, 0x04)?;
        let file_count  = get_u32(&file_data, 0x14)?;
        println!("files: {file_count}");
        // Finds the begining of the file path/name and pushes it into the file name offset vector
        let mut file_name_offsets = Vec::with_capacity(file_count as usize);
        let mut off = 0x40;
        let mut debug_counter = 0;
        let mut debug_offset = 0;
        for _ in 0..file_count {
            debug_offset = get_u32(&file_data, off)?;
            file_name_offsets.push(debug_offset);
            println!("filename offset {debug_offset:08X} {debug_counter}");
            off += 4;
            debug_counter += 1;
        }
        // Finds the path/name of the file and pushes it into the file name vector
        let mut file_names = Vec::with_capacity(file_count as usize);
        for &name_offset in &file_name_offsets {
            let name_string = read_cstr(&file_data, name_offset as usize)
                .with_context(|| format!("reading name at 0x{name_offset:08X}"))?;
            file_names.push(name_string);
        }
        // Crude way to locate the data index table but I haven't found a pointer to it yet
        let data_index_start = (*file_name_offsets.last().unwrap() as usize) + file_names.last().unwrap().len() + 5;
        println!("data index start: {data_index_start:08X}");
        // Locates the begining of each file
        let mut file_entry_data_begining: Vec<u32> = Vec::with_capacity(file_count as usize - 1);
        let mut _tmp_offset = data_index_start;
        for _ in 0..(file_count) {
            println!("{_tmp_offset:08X}");
            let file_loc = get_u32(&file_data, _tmp_offset)?; _tmp_offset += 12; //skips over the 4byte File location, 4byte file ID and the 4byte Common id
            file_entry_data_begining.push(file_loc);       
            println!("file at {file_loc:08X}");
        }
        //Create an entry vector
        let mut entries = Vec::with_capacity(file_count as usize);
        let mut data_start = *file_entry_data_begining.first().unwrap();
        let mut i: usize  = 0;
        println!("Count:{file_count}");
        println!("Count Length:{0}", file_entry_data_begining.len());
        //Pushes all but the last entry into the entry vector
        while  i < (file_count as usize - 1) {
            println!("{i}");
            let size = ((file_entry_data_begining[i + 1] - file_entry_data_begining[i]) as u64);
            let start = (file_entry_data_begining[i] as u64);
            let full_path = file_names[i].clone();
            let magic = &file_data[start as usize..start as usize + 4.min(size as usize)];
            let compression = if magic == ZSTD_MAGIC { Compression::Zstd } else { Compression::Raw };
            entries.push(GrpEntry {
                index: i as u32,
                full_path,
                start,
                size,
                compression,
            });
            i += 1;
        }
        let _temp_debug_file_data_len = file_data.len();
        println!("{_temp_debug_file_data_len:08X}");

        let size = (((file_data.len() as u32) - file_entry_data_begining[i]) as u64);
        let start = (file_entry_data_begining[i] as u64);
        let full_path = file_names[i].clone();
        let magic = &file_data[start as usize..start as usize +4.min(size as usize)];
        let compression = if magic == ZSTD_MAGIC {Compression::Zstd} else {Compression::Raw};
        entries.push(GrpEntry{
            index: i as u32,
            full_path, 
            start, 
            size, 
            compression
        });


        Ok(GrpFile {
            path: grp_path.to_path_buf(),
            file_data,
            header_size,
            file_count,
            data_start,
            entries,
        })
    }
    //Extracts the entry
    fn extract_entry(&self, entry: &GrpEntry, out_dir: &Path) -> Result<PathBuf> {
        let bytes = &self.file_data[entry.start as usize .. (entry.start + entry.size) as usize];
        let out_path = out_dir.join(&entry.full_path);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        //Handles Compression
        match entry.compression {
            Compression::Zstd => {
                let mut dec = zstd::stream::read::Decoder::new(Cursor::new(bytes))?;
                let mut out = Vec::with_capacity(bytes.len() * 2);
                dec.read_to_end(&mut out)?;
                fs::write(&out_path, out)?;
            }
            Compression::Raw => {
                fs::write(&out_path, bytes)?;
            }
        }
        Ok(out_path)
    }

    fn read_entry(&self, entry: &GrpEntry) -> Result<Vec<u8>> {
        let bytes = &self.file_data[entry.start as usize .. (entry.start + entry.size) as usize];
        match entry.compression {
            Compression::Zstd => {
                let mut dec = zstd::stream::read::Decoder::new(Cursor::new(bytes))?;
                let mut out = Vec::new();
                dec.read_to_end(&mut out)?;
                Ok(out)
            }
            Compression::Raw => Ok(bytes.to_vec()),
        }
    }
}
//Handles getting little-endian 4byte values
fn get_u32(data: &[u8], off: usize) -> Result<u32> {
    if off + 4 > data.len() { anyhow::bail!("EOF reading u32 at 0x{off:08X}"); } //doubt this will ever happen but better safe than sorry
    Ok(u32::from_le_bytes(data[off..off+4].try_into().unwrap()))
}

fn read_cstr(buf: &[u8], off: usize) -> Result<String> {
    let mut end = off;
    while end < buf.len() && buf[end] != 0 { end += 1; }
    if end == buf.len() { anyhow::bail!("unterminated string at 0x{off:08X}"); }
    Ok(std::str::from_utf8(&buf[off..end])?.to_string())
}


/* --------------------------- Not quite sure how stuff works past this point, Chat-GiPiTy ui magic --------------------------- */

#[derive(Default)]
struct AppState {
    pack: Option<GrpFile>,
    root: TreeNode,
    selected: Option<usize>,
    message: String,
    mdl_viewer: Option<ModelViewer>,
    mdl_viewer_idx: Option<usize>,
}

#[derive(Default)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    files: BTreeMap<String, usize> 
}

impl TreeNode {
    fn insert(&mut self, parts: &[&str], file_index: usize) {
        match parts {
            [] => {}
            [last] => {
                self.files.insert((*last).to_string(), file_index);
            }
            [head,rest @ ..] => {
                self.children
                    .entry((*head).to_string())
                    .or_default()
                    .insert(rest,file_index);
            }
        }
    }
}

impl AppState {
    fn build_tree(&mut self) {
        self.root = TreeNode::default();
        if let Some(pack) = &self.pack {
            for (i, e) in pack.entries.iter().enumerate() {
                let parts: Vec<&str> = e.full_path.split('/').filter(|s| !s.is_empty()).collect();
                self.root.insert(&parts, i);
            }
        }
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            if ui.add(Button::new("Open .grp")).clicked() {
                if let Some(path) = FileDialog::new().add_filter("GRP", &["grp"]).pick_file() {
                    let index = path.with_extension("index");
                    match GrpFile::parse(&path) {
                        Ok(pack) => {
                            self.message = format!(
                                "Loaded {} files from {}",
                                pack.file_count,
                                path.file_name().unwrap().to_string_lossy()
                            );
                            self.pack = Some(pack);
                            self.build_tree();
                            self.selected = None;
                        }
                        Err(e) => { self.message = format!("Failed to open: {e:#}"); }
                    }
                }
            }
            ui.label(&self.message);
        });

        egui::SidePanel::left("left").resizable(true).show(ctx, |ui| {
            if let Some(pack) = &self.pack {
                ui.heading("Files");
                egui::ScrollArea::vertical()
                .id_salt("file_tree_scroll").auto_shrink([false;2])
                .show(ui, |ui|{
                    draw_tree(ui, &self.root, pack, &mut self.selected);
                });
                
                if ui.add(Button::new("Extract All…")).clicked() {
                    if let Some(folder) = FileDialog::new().pick_folder() {
                        let mut ok = 0usize;
                        for e in &pack.entries {
                            if pack.extract_entry(e, &folder).is_ok() { ok += 1; }
                        }
                        self.message = format!("Extracted {ok}/{} files to {}", pack.entries.len(),
                                               folder.display());
                    }
                }
            } else {
                ui.label("Open a .grp to view its contents.");
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let (Some(pack), Some(sel)) = (&self.pack, self.selected) {
                let e = &pack.entries[sel];
                ui.heading("Details");
                ui.monospace(&e.full_path);
                ui.separator();
                ui.label(format!("Index: {}", e.index));
                ui.label(format!("Start: 0x{:X}", e.start));
                ui.label(format!("Size: {} bytes", e.size));
                ui.label(format!("Compression: {:?}", e.compression));
                if ui.add(Button::new("Extract this file…")).clicked() {
                    if let Some(folder) = FileDialog::new().pick_folder() {
                        match pack.extract_entry(e, &folder) {
                            Ok(p) => self.message = format!("Saved {}", p.display()),
                            Err(err) => self.message = format!("Extract failed: {err:#}"),
                        }
                    }
                }
            }
        });
        // Load model viewer when an MDL file is selected
        if let (Some(pack), Some(sel)) = (&self.pack, self.selected) {
            let entry = &pack.entries[sel];
            if entry.full_path.to_lowercase().ends_with(".mdl") {
                if self.mdl_viewer_idx != Some(sel) {
                    match pack.read_entry(entry).and_then(|d| mdl::parse_all_chunks(&d)) {
                        Ok(chunks) => {
                            self.mdl_viewer = Some(ModelViewer::new(chunks));
                            self.mdl_viewer_idx = Some(sel);
                        }
                        Err(err) => {
                            self.message = format!("Failed to load model: {err:#}");
                            self.mdl_viewer = None;
                            self.mdl_viewer_idx = None;
                        }
                    }
                }
            } else {
                self.mdl_viewer = None;
                self.mdl_viewer_idx = None;
            }
        } else {
            self.mdl_viewer = None;
            self.mdl_viewer_idx = None;
        }

        if let Some(viewer) = &mut self.mdl_viewer {
            egui::Window::new("Model Viewer")
                .anchor(Align2::RIGHT_BOTTOM, [0.0, 0.0])
                .show(ctx, |ui| { viewer.ui(ui); });
        }
    }
}

fn draw_tree(ui: &mut egui::Ui, node: &TreeNode, pack: &GrpFile, selected: &mut Option<usize>) {
    for (name, child) in &node.children {
        egui::CollapsingHeader::new(name).default_open(false).show(ui, |ui| {
            draw_tree(ui, child, pack, selected);
        });
    }
    for (name, &idx) in &node.files {
        if ui.selectable_label(*selected == Some(idx), name).clicked() {
            *selected = Some(idx);
        }
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "GRP Viewer",
        native_options,
Box::new(|_cc| {Ok::<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>>(Box::<AppState>::default())})
    )   
}
