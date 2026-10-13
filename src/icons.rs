//! Toolbar icons: the "Fluent UI" icon set of Notepad++ (GPL-3.0, `PowerEditor/src/icons/*/toolbar/
//! regular`), recoloured at load time – the Notepad++ blue becomes the accent colour of the UI
//! theme and the grey strokes take the theme's text colour.

use std::collections::HashMap;

use eframe::egui::{self, Color32, TextureHandle, TextureOptions};

macro_rules! icon_set {
    ($mode:literal: $($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../assets/toolbar/", $mode, "/", $name, ".png")) as &[u8])),*]
    };
}

type Set = &'static [(&'static str, &'static [u8])];

const LIGHT: Set = icon_set!("light":
    "new", "open", "save", "saveall", "close", "closeall", "print", "cut", "copy", "paste", "undo", "redo",
    "find", "findrep", "zoomIn", "zoomOut", "syncV", "syncH", "wrap", "allChars", "indentGuide", "udl",
    "docMap", "docList", "funcList", "fileBrowser", "monitoring", "startrecord", "stoprecord", "playrecord",
    "playrecord_m", "saverecord");
const DARK: Set = icon_set!("dark":
    "new", "open", "save", "saveall", "close", "closeall", "print", "cut", "copy", "paste", "undo", "redo",
    "find", "findrep", "zoomIn", "zoomOut", "syncV", "syncH", "wrap", "allChars", "indentGuide", "udl",
    "docMap", "docList", "funcList", "fileBrowser", "monitoring", "startrecord", "stoprecord", "playrecord",
    "playrecord_m", "saverecord");

/// Replaces the Notepad++ blue by `accent` and the neutral grey by `text` (alpha is kept, so the
/// anti-aliasing stays intact).
pub fn recolor(rgba: &mut [u8], accent: Color32, text: Color32) {
    for px in rgba.chunks_exact_mut(4) {
        if px[3] == 0 {
            continue;
        }
        let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
        let c = if b - r > 60 {
            accent
        } else if (r - g).abs() < 12 && (g - b).abs() < 12 {
            text
        } else {
            continue;
        };
        px[0] = c.r();
        px[1] = c.g();
        px[2] = c.b();
    }
}

/// Textures for the current mode (rebuilt when dark mode changes).
#[derive(Default)]
pub struct Icons {
    dark: Option<bool>,
    tex: HashMap<&'static str, TextureHandle>,
}

impl Icons {
    pub fn get(&mut self, ctx: &egui::Context, dark: bool, name: &str) -> Option<egui::TextureId> {
        if self.dark != Some(dark) {
            self.dark = Some(dark);
            self.tex.clear();
            let accent = crate::theme::accent(dark);
            let text = crate::theme::visuals(dark).text_color();
            for (n, png) in if dark { DARK } else { LIGHT } {
                let Ok(img) = eframe::icon_data::from_png_bytes(png) else { continue };
                let mut rgba = img.rgba;
                recolor(&mut rgba, accent, text);
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &rgba);
                self.tex.insert(n, ctx.load_texture(format!("tb_{n}"), ci, TextureOptions::LINEAR));
            }
        }
        self.tex.get(name).map(|t| t.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icons_decode_and_recolor() {
        for set in [LIGHT, DARK] {
            for (n, png) in set {
                let img = eframe::icon_data::from_png_bytes(png).unwrap_or_else(|e| panic!("{n}: {e}"));
                assert_eq!((img.width, img.height), (32, 32), "{n}");
            }
        }
        let accent = Color32::from_rgb(184, 65, 27);
        let text = Color32::from_rgb(31, 27, 24);
        let mut px = vec![0, 120, 212, 255, 33, 33, 33, 128, 0, 120, 212, 0, 200, 40, 40, 255];
        recolor(&mut px, accent, text);
        assert_eq!(px, vec![184, 65, 27, 255, 31, 27, 24, 128, 0, 120, 212, 0, 200, 40, 40, 255]);
        // No blue left in any recoloured icon.
        let img = eframe::icon_data::from_png_bytes(LIGHT.iter().find(|(n, _)| *n == "save").unwrap().1).unwrap();
        let mut rgba = img.rgba;
        recolor(&mut rgba, accent, text);
        assert!(!rgba.chunks_exact(4).any(|p| p[3] > 0 && p[2] as i32 - p[0] as i32 > 60));
    }
}
