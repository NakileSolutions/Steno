//! Titres des fenêtres par X11 (EWMH : `_NET_CLIENT_LIST`, `_NET_WM_PID`, `_NET_WM_NAME`,
//! `_NET_ACTIVE_WINDOW`). Sous Wayland, seules les apps qui passent par XWayland (souvent les
//! apps Electron : Teams for Linux, Slack, Discord…) sont visibles ; sinon le titre reste
//! inconnu (`"title": null`, prévu par le contrat).

use std::collections::HashSet;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

use crate::titles::WindowCandidate;

pub struct X11 {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
}

struct Atoms {
    client_list: u32,
    wm_pid: u32,
    net_wm_name: u32,
    utf8: u32,
    active: u32,
}

impl X11 {
    pub fn connect() -> Option<X11> {
        std::env::var_os("DISPLAY")?;
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atom = |n: &str| -> Option<u32> { Some(conn.intern_atom(false, n.as_bytes()).ok()?.reply().ok()?.atom) };
        let atoms = Atoms {
            client_list: atom("_NET_CLIENT_LIST")?,
            wm_pid: atom("_NET_WM_PID")?,
            net_wm_name: atom("_NET_WM_NAME")?,
            utf8: atom("UTF8_STRING")?,
            active: atom("_NET_ACTIVE_WINDOW")?,
        };
        Some(X11 { conn, root, atoms })
    }

    fn u32s(&self, w: Window, prop: u32, ty: impl Into<u32>) -> Vec<u32> {
        self.conn
            .get_property(false, w, prop, ty.into(), 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn title(&self, w: Window) -> Option<String> {
        let get = |prop: u32, ty: u32| -> Option<String> {
            let r = self.conn.get_property(false, w, prop, ty, 0, 1024).ok()?.reply().ok()?;
            let s = String::from_utf8_lossy(&r.value).trim().to_string();
            (!s.is_empty()).then_some(s)
        };
        get(self.atoms.net_wm_name, self.atoms.utf8).or_else(|| get(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
    }

    /// Fenêtres de premier niveau appartenant aux processus `pids`.
    pub fn windows(&self, pids: &HashSet<u32>) -> Vec<WindowCandidate> {
        if pids.is_empty() {
            return vec![];
        }
        let active = self.u32s(self.root, self.atoms.active, AtomEnum::WINDOW).first().copied();
        let mut seen_pid: HashSet<u32> = HashSet::new();
        let mut out = vec![];
        for w in self.u32s(self.root, self.atoms.client_list, AtomEnum::WINDOW) {
            let Some(pid) = self.u32s(w, self.atoms.wm_pid, AtomEnum::CARDINAL).first().copied() else { continue };
            if !pids.contains(&pid) {
                continue;
            }
            let Some(title) = self.title(w) else { continue };
            // La première fenêtre (ordre de création) d'un processus fait office de fenêtre principale.
            let is_main = seen_pid.insert(pid);
            out.push(WindowCandidate { title, is_main, is_focused: Some(w) == active });
        }
        let _ = self.conn.flush();
        out
    }
}
