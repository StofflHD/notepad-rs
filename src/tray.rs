//! System tray icon (freedesktop StatusNotifierItem via D-Bus, shown by KDE, Xfce, Cinnamon,
//! LXQt, Budgie …; GNOME needs the AppIndicator extension).
//!
//! The icon only exists while the window is hidden in the tray, like in Notepad++.

use std::sync::mpsc::Sender;

use eframe::egui;
use ksni::blocking::TrayMethods;

use crate::i18n::tr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Show,
    New,
    Open,
    Exit,
}

struct Tray {
    tx: Sender<Event>,
    ctx: egui::Context,
    icon: Vec<ksni::Icon>,
    tooltip: String,
}

impl Tray {
    fn send(&self, ev: Event) {
        let _ = self.tx.send(ev);
        // The window is hidden: make it visible again first, otherwise it gets no frames.
        if ev != Event::Exit {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        self.ctx.request_repaint();
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "notepad-rs".into()
    }

    fn title(&self) -> String {
        crate::app::APP_NAME.into()
    }

    fn icon_name(&self) -> String {
        "notepad-rs".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: crate::app::APP_NAME.into(), description: self.tooltip.clone(), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Event::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let item = |label: &str, icon: &str, ev: Event| -> ksni::MenuItem<Self> {
            StandardItem { label: tr(label).replace('_', "__"), icon_name: icon.into(), activate: Box::new(move |t: &mut Self| t.send(ev)), ..Default::default() }.into()
        };
        vec![
            item("Activate NotepadRS", "notepad-rs", Event::Show),
            ksni::MenuItem::Separator,
            item("New", "document-new", Event::New),
            item("Open...", "document-open", Event::Open),
            ksni::MenuItem::Separator,
            item("Exit", "application-exit", Event::Exit),
        ]
    }
}

/// ARGB32 (network byte order) icons from the embedded PNG, in a few sizes.
fn icons() -> Vec<ksni::Icon> {
    let Ok(img) = eframe::icon_data::from_png_bytes(include_bytes!("../packaging/notepad-rs.png")) else { return Vec::new() };
    let (w, h) = (img.width as usize, img.height as usize);
    [22usize, 32, 48, 64]
        .iter()
        .map(|&s| {
            // Box filter down-scaling of the 256 px RGBA image.
            let mut data = Vec::with_capacity(s * s * 4);
            for y in 0..s {
                for x in 0..s {
                    let (x0, x1) = (x * w / s, ((x + 1) * w / s).max(x * w / s + 1));
                    let (y0, y1) = (y * h / s, ((y + 1) * h / s).max(y * h / s + 1));
                    let mut acc = [0u32; 4];
                    for yy in y0..y1 {
                        for xx in x0..x1 {
                            let p = &img.rgba[(yy * w + xx) * 4..][..4];
                            let a = p[3] as u32;
                            acc[0] += p[0] as u32 * a;
                            acc[1] += p[1] as u32 * a;
                            acc[2] += p[2] as u32 * a;
                            acc[3] += a;
                        }
                    }
                    let n = ((x1 - x0) * (y1 - y0)) as u32;
                    let a = acc[3];
                    let c = |v: u32| v.checked_div(a).unwrap_or(0) as u8;
                    data.extend_from_slice(&[(a / n) as u8, c(acc[0]), c(acc[1]), c(acc[2])]);
                }
            }
            ksni::Icon { width: s as i32, height: s as i32, data }
        })
        .collect()
}

pub struct Handle(ksni::blocking::Handle<Tray>);

impl Handle {
    pub fn remove(self) {
        self.0.shutdown().wait();
    }
}

/// Shows the tray icon. Fails when no StatusNotifier host (tray) is running.
pub fn show(ctx: &egui::Context, tx: Sender<Event>, tooltip: String) -> Result<Handle, String> {
    let tray = Tray { tx, ctx: ctx.clone(), icon: icons(), tooltip };
    tray.spawn().map(Handle).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_sizes() {
        let icons = super::icons();
        assert_eq!(icons.iter().map(|i| i.width).collect::<Vec<_>>(), vec![22, 32, 48, 64]);
        assert!(icons.iter().all(|i| i.data.len() == (i.width * i.height * 4) as usize));
        // Corners of the squircle are transparent, the centre is opaque.
        let i = &icons[3];
        assert_eq!(i.data[0], 0);
        let c = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(i.data[c], 255);
    }
}
