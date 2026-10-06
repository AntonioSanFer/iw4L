//! In-game folder browser behind `options_game_folder_browser`. Android has no
//! rfd backend, and its system picker hands out `content://` URIs the asset
//! loader cannot open, so the menu walks real paths with `std::fs`.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use frame::{OtherGame, UiMenuDvars};

use crate::game_folders::{FolderPicks, tail, zone_game};
use crate::{CommandSpec, ConsoleCommand, ConsoleRegistry};

/// Matches the `row_<n>` items in `ui/menus/game_folder_browser.json`.
const ROWS: usize = 12;

#[derive(Default)]
pub(crate) struct BrowserState {
    game: Option<OtherGame>,
    /// `None` lists the storage roots.
    dir: Option<PathBuf>,
    entries: Vec<(String, PathBuf)>,
    page: usize,
    unreadable: bool,
}

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for name in [
        "ui_folders_open",
        "ui_folders_enter",
        "ui_folders_up",
        "ui_folders_page",
        "ui_folders_use",
    ] {
        registry.register(CommandSpec::new(name));
    }
}

#[cfg(target_os = "android")]
fn storage_roots() -> Vec<(String, PathBuf)> {
    let mut roots = vec![(
        "Internal storage".to_owned(),
        PathBuf::from("/storage/emulated/0"),
    )];
    // Removable volumes mount as `/storage/XXXX-XXXX`.
    if let Ok(entries) = std::fs::read_dir("/storage") {
        let mut cards: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                (!matches!(name.as_str(), "emulated" | "self") && path.is_dir())
                    .then(|| (format!("SD card ({name})"), path))
            })
            .collect();
        cards.sort();
        roots.extend(cards);
    }
    roots
}

#[cfg(not(target_os = "android"))]
fn storage_roots() -> Vec<(String, PathBuf)> {
    if cfg!(windows) {
        ('C'..='Z')
            .map(|letter| (format!("{letter}:"), PathBuf::from(format!("{letter}:\\"))))
            .filter(|(_, drive)| drive.is_dir())
            .collect()
    } else {
        vec![("/".to_owned(), PathBuf::from("/"))]
    }
}

fn subfolders(dir: &Path) -> Option<Vec<(String, PathBuf)>> {
    let mut folders: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            (!name.starts_with('.') && path.is_dir()).then_some((name, path))
        })
        .collect();
    folders.sort_by_key(|(name, _)| name.to_lowercase());
    Some(folders)
}

impl BrowserState {
    fn show(&mut self, dir: Option<PathBuf>) {
        self.page = 0;
        self.unreadable = false;
        self.entries = match &dir {
            None => storage_roots(),
            Some(dir) => subfolders(dir).unwrap_or_else(|| {
                self.unreadable = true;
                Vec::new()
            }),
        };
        self.dir = dir;
    }

    fn up(&mut self) {
        let Some(dir) = &self.dir else {
            return;
        };
        let at_root = storage_roots().iter().any(|(_, root)| root == dir);
        let parent = (!at_root).then(|| dir.parent()).flatten();
        self.show(parent.map(Path::to_path_buf));
    }

    fn pages(&self) -> usize {
        self.entries.len().div_ceil(ROWS).max(1)
    }

    fn status(&self, game: OtherGame) -> String {
        let Some(dir) = &self.dir else {
            return "Pick a storage location.".to_owned();
        };
        if self.unreadable {
            return if cfg!(target_os = "android") {
                "^1Cannot read this folder.^7 Allow \"All files access\" for iw4l in Android settings."
                    .to_owned()
            } else {
                "^1Cannot read this folder.".to_owned()
            };
        }
        let root = asset_transport::game_install_root(dir);
        if asset_transport::folder_holds_game(&root, zone_game(game)) {
            format!(
                "^2{} content found here.^7 Press Use This Folder.",
                game.title()
            )
        } else if self.entries.is_empty() {
            "No subfolders.".to_owned()
        } else {
            String::new()
        }
    }

    fn publish(&self, game: OtherGame, dvars: &mut UiMenuDvars) {
        const PATH_WIDTH: usize = 72;
        dvars.set(
            "ui_folders_title",
            format!("Choose the {} folder", game.title()),
        );
        let path = self.dir.as_ref().map_or_else(
            || "Storage".to_owned(),
            |dir| tail(&dir.display().to_string(), PATH_WIDTH),
        );
        dvars.set("ui_folders_path", path);
        dvars.set("ui_folders_status", self.status(game));
        let pages = self.pages();
        dvars.set(
            "ui_folders_page_label",
            if pages > 1 {
                format!("Page {}/{pages}", self.page + 1)
            } else {
                String::new()
            },
        );
        dvars.set("ui_folders_paged", flag(pages > 1));
        dvars.set("ui_folders_can_up", flag(self.dir.is_some()));
        dvars.set("ui_folders_can_use", flag(self.dir.is_some()));
        let shown = self.entries.iter().skip(self.page * ROWS);
        let mut shown = shown.map(|(name, _)| name.as_str());
        for row in 0..ROWS {
            let name = shown.next();
            dvars.set(&format!("ui_folders_row_{row}"), name.unwrap_or(""));
            dvars.set(
                &format!("ui_folders_row_{row}_visible"),
                flag(name.is_some()),
            );
        }
    }
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

pub(crate) fn route(
    mut commands: MessageReader<ConsoleCommand>,
    picks: Res<FolderPicks>,
    settings: Res<frame::GameSettings>,
    mut state: Local<BrowserState>,
    mut dvars: ResMut<UiMenuDvars>,
) {
    let mut changed = false;
    for command in commands
        .read()
        .filter(|command| command.name.starts_with("ui_folders_"))
    {
        let arg = command.args.first().map(String::as_str).unwrap_or("");
        match command.name.as_str() {
            "ui_folders_open" => {
                let Some(game) = OtherGame::from_key(arg) else {
                    warn!("unknown game folder key `{arg}`");
                    continue;
                };
                let current = PathBuf::from(settings.game_folder(game));
                let start = if current.is_dir() {
                    Some(current)
                } else {
                    asset_transport::games_root_from_env()
                        .ok()
                        .map(|root| root.0)
                        .filter(|root| root.is_dir())
                };
                state.game = Some(game);
                state.show(start);
            }
            "ui_folders_enter" => {
                let picked = arg
                    .parse::<usize>()
                    .ok()
                    .filter(|&row| row < ROWS)
                    .and_then(|row| state.entries.get(state.page * ROWS + row))
                    .map(|(_, path)| path.clone());
                if let Some(path) = picked {
                    state.show(Some(path));
                }
            }
            "ui_folders_up" => state.up(),
            "ui_folders_page" => {
                if let Ok(delta) = arg.parse::<isize>() {
                    let pages = state.pages();
                    state.page = (state.page as isize + delta).rem_euclid(pages as isize) as usize;
                }
            }
            "ui_folders_use" => {
                if let (Some(game), Some(dir)) = (state.game, state.dir.clone()) {
                    picks.deliver(game, dir);
                }
            }
            _ => continue,
        }
        changed = true;
    }
    if changed && let Some(game) = state.game {
        state.publish(game, &mut dvars);
    }
}
