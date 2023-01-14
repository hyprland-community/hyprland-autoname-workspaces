use clap::Parser;
use core::str;
use hyprland::data::Clients;
use hyprland::dispatch::*;
use hyprland::event_listener::EventListenerMutable as EventListener;
use hyprland::prelude::*;
use hyprland::shared::WorkspaceType;
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Mutex;

lazy_static! {
    static ref ICONS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("bleachbit", "");
        m.insert("calibre", "");
        m.insert("chromium", "");
        m.insert("code-oss", "");
        m.insert("discord", "");
        m.insert("draw.io", "");
        m.insert("firefox", "");
        m.insert("gcr-prompter", "");
        m.insert("kitty", "");
        m.insert("krita", "");
        m.insert("libreoffice-calc", "");
        m.insert("libreoffice-writer", "");
        m.insert("microsoft teams - preview", "");
        m.insert("mpv", "");
        m.insert("neomutt", "");
        m.insert("org.ksnip.ksnip", "");
        m.insert("org.pwmt.zathura", "");
        m.insert("org.qutebrowser.qutebrowser", "");
        m.insert("personal", "");
        m.insert("work", "");
        m.insert("paperwork", "");
        m.insert("pavucontrol", "");
        m.insert("peek", "");
        m.insert("qutepreview", "");
        m.insert("riot", "");
        m.insert("scli", "");
        m.insert("signal", "");
        m.insert("slack", "");
        m.insert("spotify", "");
        m.insert("transmission-gtk", "");
        m.insert("vimiv", "");
        m.insert("virt-manager", "");
        m.insert("wofi", "");
        m.insert("xplr", "");
        m.insert("nemo", "");
        m.insert("nautilus", "");
        m.insert("DEFAULT", "");
        m
    };
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    dedup: bool,
}

fn main() -> hyprland::shared::HResult<()> {
    // Parse cli
    let args = Args::parse();
    // Init
    let r = Rc::new(Renamer::new(args));
    let _ = &r.renameworkspace();
    // Run on window events
    r.start_listeners()
}

struct Renamer {
    workspaces: Mutex<HashSet<i32>>,
    args: Args,
}

impl Renamer {
    fn new(args: Args) -> Self {
        let workspaces = Mutex::new(HashSet::new());
        Renamer { workspaces, args }
    }

    fn removeworkspace(&self, wt: WorkspaceType) {
        match wt {
            WorkspaceType::Unnamed(x) => self.workspaces.lock().unwrap().remove(&x),
            WorkspaceType::Special(_) => false,
            WorkspaceType::Named(_) => false,
        };
    }

    fn renameworkspace(&self) {
        let clients = Clients::get().unwrap();
        let mut deduper: HashSet<String> = HashSet::new();
        let mut workspaces = self
            .workspaces
            .lock()
            .unwrap()
            .iter()
            .map(|&c| (c, "".to_string()))
            .collect::<HashMap<_, _>>();

        for client in clients.collect().iter() {
            let class = client.clone().class.to_lowercase();
            let fullscreen = client.fullscreen;
            let icon = class_to_icon(&class).to_string();
            let workspace_id = client.clone().workspace.id;
            let is_dup = !deduper.insert(format!("{}{}", workspace_id.clone(), icon));

            self.workspaces
                .lock()
                .unwrap()
                .insert(client.clone().workspace.id);

            let workspace = workspaces
                .entry(workspace_id)
                .or_insert(format!(" {}", icon));

            if fullscreen && !self.args.dedup {
                *workspace = format!("{} [{}]", workspace, icon);
            } else if fullscreen && self.args.dedup && is_dup {
                *workspace =
                    workspace.replace(icon.as_str(), format!("[{}]", icon.as_str()).as_str());
            } else if self.args.dedup && is_dup {
                *workspace = workspace.to_string();
            } else {
                *workspace = format!("{} {}", workspace, icon);
            }
        }

        for (id, apps) in workspaces.clone().into_iter() {
            rename_cmd(id, &apps);
        }
    }

    fn start_listeners(self: &Rc<Self>) -> hyprland::shared::HResult<()> {
        let mut event_listener = EventListener::new();

        let this = self.clone();
        event_listener.add_window_open_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_window_moved_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_window_close_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_added_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_moved_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_change_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_fullscreen_state_change_handler(move |_, _| this.renameworkspace());
        let this = self.clone();
        event_listener.add_workspace_destroy_handler(move |wt, _| {
            this.renameworkspace();
            this.removeworkspace(wt);
        });

        event_listener.start_listener()
    }
}

fn class_to_icon(class: &str) -> &str {
    return ICONS
        .get(&class)
        .unwrap_or_else(|| ICONS.get("DEFAULT").unwrap());
}

fn rename_cmd(id: i32, apps: &str) {
    let text = format!("{}:{}", id.clone(), apps);
    let content = (!apps.is_empty()).then_some(text.as_str());
    hyprland::dispatch!(RenameWorkspace, id, content).unwrap();
}
