//! Project panels: workspaces in Notepad++'s project XML format
//! (`<NotepadPlus><Project name=".."><Folder name=".."><File name="rel/path"/>…`).

use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Folder { name: String, children: Vec<Node> },
    File { path: PathBuf },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workspace {
    pub path: Option<PathBuf>,
    /// Top-level nodes are projects (folders).
    pub projects: Vec<Node>,
    pub dirty: bool,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn rel(path: &Path, base: Option<&Path>) -> String {
    match base.and_then(|b| path.strip_prefix(b).ok()) {
        Some(r) => r.display().to_string(),
        None => path.display().to_string(),
    }
}

impl Workspace {
    pub fn load(path: &Path) -> Result<Self, String> {
        let xml = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let base = path.parent().map(Path::to_path_buf);
        let mut r = Reader::from_str(&xml);
        let mut stack: Vec<(String, Vec<Node>)> = vec![(String::new(), Vec::new())];
        let attr = |e: &quick_xml::events::BytesStart, n: &str| -> String {
            e.attributes()
                .flatten()
                .find(|a| a.key.as_ref() == n.as_bytes())
                .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
                .unwrap_or_default()
        };
        loop {
            match r.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) if matches!(e.name().as_ref(), b"Project" | b"Folder") => stack.push((attr(&e, "name"), Vec::new())),
                Event::Empty(e) if matches!(e.name().as_ref(), b"Project" | b"Folder") => {
                    stack.last_mut().unwrap().1.push(Node::Folder { name: attr(&e, "name"), children: Vec::new() })
                }
                Event::End(e) if matches!(e.name().as_ref(), b"Project" | b"Folder") => {
                    if stack.len() > 1 {
                        let (name, children) = stack.pop().unwrap();
                        stack.last_mut().unwrap().1.push(Node::Folder { name, children });
                    }
                }
                Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"File" => {
                    let name = attr(&e, "name").replace('\\', "/");
                    let p = PathBuf::from(&name);
                    let p = if p.is_absolute() { p } else { base.as_ref().map_or(p.clone(), |b| b.join(&p)) };
                    stack.last_mut().unwrap().1.push(Node::File { path: p });
                }
                Event::Eof => break,
                _ => {}
            }
        }
        Ok(Self { path: Some(path.to_path_buf()), projects: stack.pop().unwrap().1, dirty: false })
    }

    pub fn to_xml(&self) -> String {
        let base = self.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
        fn write(n: &Node, depth: usize, base: Option<&Path>, out: &mut String, top: bool) {
            let ind = "    ".repeat(depth);
            match n {
                Node::Folder { name, children } => {
                    let tag = if top { "Project" } else { "Folder" };
                    out.push_str(&format!("{ind}<{tag} name=\"{}\">\n", esc(name)));
                    for c in children {
                        write(c, depth + 1, base, out, false);
                    }
                    out.push_str(&format!("{ind}</{tag}>\n"));
                }
                Node::File { path } => out.push_str(&format!("{ind}<File name=\"{}\" />\n", esc(&rel(path, base)))),
            }
        }
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<NotepadPlus>\n");
        for p in &self.projects {
            write(p, 1, base.as_deref(), &mut out, true);
        }
        out.push_str("</NotepadPlus>\n");
        out
    }

    pub fn save(&mut self) -> Result<(), String> {
        let p = self.path.clone().ok_or("No workspace file")?;
        std::fs::write(&p, self.to_xml()).map_err(|e| e.to_string())?;
        self.dirty = false;
        Ok(())
    }

    /// Node at a path of child indices.
    pub fn node_mut(&mut self, at: &[usize]) -> Option<&mut Node> {
        let (first, rest) = at.split_first()?;
        let mut n = self.projects.get_mut(*first)?;
        for &i in rest {
            n = match n {
                Node::Folder { children, .. } => children.get_mut(i)?,
                Node::File { .. } => return None,
            };
        }
        Some(n)
    }

    pub fn remove(&mut self, at: &[usize]) {
        if at.len() == 1 {
            if at[0] < self.projects.len() {
                self.projects.remove(at[0]);
            }
        } else if let Some(Node::Folder { children, .. }) = self.node_mut(&at[..at.len() - 1]) {
            let i = at[at.len() - 1];
            if i < children.len() {
                children.remove(i);
            }
        }
        self.dirty = true;
    }

    /// Moves a node up (-1) or down (+1) among its siblings.
    pub fn move_node(&mut self, at: &[usize], delta: isize) {
        let i = at[at.len() - 1];
        let list = if at.len() == 1 {
            &mut self.projects
        } else {
            match self.node_mut(&at[..at.len() - 1]) {
                Some(Node::Folder { children, .. }) => children,
                _ => return,
            }
        };
        let j = i as isize + delta;
        if j >= 0 && (j as usize) < list.len() {
            list.swap(i, j as usize);
            self.dirty = true;
        }
    }
}

/// Adds all files of a directory (recursively) as folder nodes.
pub fn folder_from_dir(dir: &Path) -> Node {
    let mut children = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                children.push(folder_from_dir(&p));
            } else {
                children.push(Node::File { path: p });
            }
        }
    }
    Node::Folder { name: dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned()), children }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = std::env::temp_dir().join(format!("nrs-proj-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ws_path = dir.join("ws.xml");
        let ws = Workspace {
            path: Some(ws_path.clone()),
            projects: vec![Node::Folder {
                name: "P & Q".into(),
                children: vec![Node::Folder { name: "src".into(), children: vec![Node::File { path: dir.join("src/main.rs") }] }, Node::File { path: PathBuf::from("/etc/hosts") }],
            }],
            dirty: false,
        };
        let xml = ws.to_xml();
        assert!(xml.contains("<File name=\"src/main.rs\" />"), "{xml}");
        std::fs::write(&ws_path, &xml).unwrap();
        let back = Workspace::load(&ws_path).unwrap();
        assert_eq!(back.projects, ws.projects);
        let mut w = back;
        w.move_node(&[0, 1], -1);
        assert!(matches!(w.node_mut(&[0, 0]), Some(Node::File { .. })));
        w.remove(&[0, 0]);
        assert!(matches!(w.node_mut(&[0, 0]), Some(Node::Folder { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
