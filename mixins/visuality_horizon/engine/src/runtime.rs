//! The one runtime of the window's client, reached by the game's hooks.
//!
//! Every hook that reaches it runs on the window's thread (ticks, packets,
//! the ambient block pass, frames and drawing), so the state lives in a
//! thread-local: no locks, and a server thread can never share it.

use crate::config::{self, Config};
use crate::engine::Engine;
use crate::settings::{Menu, Outcome};
use std::cell::RefCell;
use std::path::PathBuf;

pub struct Runtime {
    pub engine: Engine,
    pub menu: Menu,
    config_path: Option<PathBuf>,
    /// One line for the settings page when the file had problems.
    pub problem: Option<String>,
    read_only: bool,
    save_failed_once: bool,
    resolved_session: Option<u64>,
    #[cfg(feature = "gpu")]
    pub gpu: crate::render::Gpu,
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Runs `f` with the runtime, creating it on first use from the config in
/// `config_dir()` (the game's shared `config/` folder). None if re-entered.
pub fn with<R>(
    config_dir: impl FnOnce() -> Option<PathBuf>,
    f: impl FnOnce(&mut Runtime) -> R,
) -> Option<R> {
    RUNTIME.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        let rt = slot.get_or_insert_with(|| Runtime::load(config_dir()));
        Some(f(rt))
    })
}

/// The runtime only if it exists already (hot paths that must not load files).
pub fn with_existing<R>(f: impl FnOnce(&mut Runtime) -> R) -> Option<R> {
    RUNTIME.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        slot.as_mut().map(f)
    })
}

/// Horizon's logger keeps only targets that start with `horizon`.
pub const LOG_TARGET: &str = "horizon::visuality_horizon";

fn log(msg: &str) {
    log::warn!(target: LOG_TARGET, "[{}] {msg}", crate::MOD_ID);
}

impl Runtime {
    pub fn load(config_dir: Option<PathBuf>) -> Runtime {
        let path = config_dir.map(|d| d.join(format!("{}.toml", crate::MOD_ID)));
        let (config, problem, read_only) = match &path {
            Some(p) => {
                let l = config::load(p);
                for i in &l.issues {
                    log(&format!(
                        "config {}: {}",
                        if i.path.is_empty() { "file" } else { &i.path },
                        i.message
                    ));
                }
                let problem = (!l.issues.is_empty())
                    .then(|| format!("{} setting(s) in the file were invalid", l.issues.len()));
                let missing = !p.exists();
                let rt = (l.config, problem, l.read_only);
                if missing {
                    // Write the documented defaults once so players can find them.
                    let _ = config::save(p, &rt.0);
                }
                rt
            }
            None => (
                Config::default(),
                Some("settings can't be saved".into()),
                true,
            ),
        };
        let mut engine = Engine::new(config);
        match crate::assets::load(crate::assets::ATLAS_JSON, crate::assets::ATLAS_PNG) {
            Ok(atlas) => engine.set_atlas(std::sync::Arc::new(atlas)),
            Err(e) => {
                log(&format!("effects are off: {e}"));
                engine.visual_failure(e);
            }
        }
        Runtime {
            engine,
            menu: Menu::default(),
            config_path: path,
            problem,
            read_only,
            save_failed_once: false,
            resolved_session: None,
            #[cfg(feature = "gpu")]
            gpu: crate::render::Gpu::default(),
        }
    }

    /// Whether tags must be resolved again (new session or config change).
    pub fn needs_resolve(&self) -> bool {
        self.resolved_session != Some(self.engine.session())
    }
    pub fn resolved(&mut self, unknown: Vec<String>) {
        self.resolved_session = Some(self.engine.session());
        if !unknown.is_empty() {
            log(&format!(
                "unknown block tags in effects.ores.tags: {}",
                unknown.join(", ")
            ));
        }
    }

    pub fn settings_labels(&self) -> Vec<String> {
        let problem = self.engine.failure.as_deref().or(self.problem.as_deref());
        let mut labels = self.menu.labels(&self.engine.config, problem);
        if self.engine.config.show_counters && self.menu.page == crate::settings::Page::Advanced {
            labels.push(self.engine.counters.summary(self.engine.active_len()));
        }
        labels
    }

    pub fn settings_click(&mut self, button: usize) {
        let mut c = self.engine.config.clone();
        match self.menu.click(&mut c, button) {
            Outcome::Changed => {
                self.engine.config = c;
                self.engine.config_changed();
                self.resolved_session = None;
                self.save();
            }
            Outcome::Reload => self.reload(),
            Outcome::Navigate | Outcome::Nothing => {}
        }
    }

    pub fn reload(&mut self) {
        let Some(p) = &self.config_path else { return };
        let l = config::load(p);
        for i in &l.issues {
            log(&format!("config {}: {}", i.path, i.message));
        }
        self.problem = (!l.issues.is_empty())
            .then(|| format!("{} setting(s) in the file were invalid", l.issues.len()));
        self.read_only = l.read_only;
        self.engine.config = l.config;
        self.engine.config_changed();
        self.resolved_session = None;
    }

    fn save(&mut self) {
        if self.read_only {
            if !self.save_failed_once {
                self.save_failed_once = true;
                log(
                    "the settings file was written by a newer version: changes are kept for this session only",
                );
            }
            return;
        }
        let Some(p) = &self.config_path else { return };
        if let Err(e) = config::save(p, &self.engine.config) {
            self.problem = Some("settings could not be saved".into());
            if !self.save_failed_once {
                self.save_failed_once = true;
                log(&format!("could not save {}: {e}", p.display()));
            }
        }
    }
}

#[cfg(feature = "gpu")]
impl Runtime {
    /// Draws this frame's particles into the world's colour target.
    pub fn draw(&mut self, target: crate::render::Target<'_>) {
        let Some(atlas) = self.engine.atlas().cloned() else {
            return;
        };
        let start = std::time::Instant::now();
        self.gpu.debug.trace = self.engine.config.collect_frame_trace;
        let (instances, counters) = self.engine.draw_parts();
        self.gpu.draw(&atlas, instances, target, counters);
        counters.last_draw_us = start.elapsed().as_micros().min(u32::MAX as u128) as u32;
    }
}
