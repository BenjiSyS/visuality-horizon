//! The Options > Mods > Visuality page: a few pages of buttons, built from
//! the validated config. Each click changes one value and is saved.

use crate::config::{Config, Family, MinimalMode, Preset};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Effects,
    Effect(Family),
    Advanced,
}

/// What a click asks of the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Only the page changed.
    Navigate,
    /// The config changed: apply and save it.
    Changed,
    /// Reload the config file from disk.
    Reload,
    Nothing,
}

pub struct Menu {
    pub page: Page,
    confirm_reset: bool,
}

impl Default for Menu {
    fn default() -> Self {
        Menu {
            page: Page::Main,
            confirm_reset: false,
        }
    }
}

const AMOUNTS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
const DISTANCES: [f32; 7] = [12.0, 16.0, 24.0, 32.0, 40.0, 48.0, 64.0];
const BUDGETS: [u32; 6] = [64, 128, 256, 384, 512, 1024];
const SPAWNS: [u32; 5] = [12, 24, 48, 80, 128];
const RADII: [i32; 5] = [4, 8, 12, 16, 24];
const SAMPLES: [u32; 5] = [5, 10, 16, 24, 32];
/// Slime tints: name and 0xRRGGBB. A colour from the file that isn't here
/// shows as "Custom" and the next click moves to the first entry.
const SLIME_COLORS: [(&str, u32); 6] = [
    ("Classic green", 0x88FF79),
    ("Lime", 0xB8FF5A),
    ("Mint", 0x7AFFC4),
    ("Sky", 0x7AC8FF),
    ("Pink", 0xFF8FD0),
    ("Amber", 0xFFC86A),
];

fn on(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}
fn pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}
fn next<T: Copy + PartialEq + PartialOrd>(list: &[T], cur: T) -> T {
    // The first entry above the current value, wrapping to the first.
    list.iter().copied().find(|&v| v > cur).unwrap_or(list[0])
}

impl Menu {
    pub fn labels(&self, c: &Config, problem: Option<&str>) -> Vec<String> {
        let mut b = Vec::new();
        match self.page {
            Page::Main => {
                b.push(format!("Visuality: {}", on(c.enabled)));
                b.push(format!("Quality preset: {}", c.preset.label()));
                b.push(format!(
                    "Particle amount: {}",
                    pct(c.quality.global_intensity)
                ));
                b.push(format!(
                    "Effect distance: {} blocks",
                    c.quality.emission_distance_blocks.round() as i32
                ));
                b.push(format!(
                    "Reduced motion and flashes: {}",
                    on(c.reduced_motion)
                ));
                b.push("Effects...".into());
                b.push("Advanced limits...".into());
                b.push(if self.confirm_reset {
                    "Click again to reset everything".into()
                } else {
                    "Reset to defaults".into()
                });
                if let Some(p) = problem {
                    b.push(format!("Note: {p}"));
                }
            }
            Page::Effects => {
                for f in Family::ALL {
                    b.push(format!("{}: {}...", f.label(), on(c.family(f).enabled)));
                }
                b.push("Back".into());
            }
            Page::Effect(f) => {
                let fam = c.family(f);
                b.push(format!("{}: {}", f.label(), on(fam.enabled)));
                b.push(format!("Amount: {}", pct(fam.intensity)));
                match f {
                    Family::Slime => {
                        let name = SLIME_COLORS
                            .iter()
                            .find(|(_, rgb)| *rgb == c.slime_color)
                            .map_or("Custom", |(n, _)| n);
                        b.push(format!("Colour: {name}"));
                    }
                    Family::Amethyst => b.push(format!(
                        "Only full-grown clusters: {}",
                        on(c.amethyst_full_clusters_only)
                    )),
                    Family::Armor => b.push(format!(
                        "Own armor in first person: {}",
                        on(c.armor_show_local_first_person)
                    )),
                    Family::Ores => b.push(format!(
                        "Only uncovered faces: {}",
                        on(c.ores_exposed_faces_only)
                    )),
                    Family::Soul => {
                        b.push(format!("Wisps above soul blocks: {}", on(c.soul_ambient)));
                        b.push(format!("Wisps when sprinting: {}", on(c.soul_sprinting)));
                    }
                    Family::Water => {
                        b.push(format!("Search radius: {} blocks", c.water_radius));
                        b.push(format!("Ripples per tick: up to {}", c.water_samples));
                        b.push(format!("Biome water colour: {}", on(c.water_biome_tint)));
                    }
                    Family::Hit | Family::Charge => {}
                }
                if matches!(f, Family::Hit | Family::Armor | Family::Ores) {
                    b.push("Lists: edit config/visuality_horizon.toml, then reload".into());
                    b.push("Reload settings file".into());
                }
                b.push("Back".into());
            }
            Page::Advanced => {
                b.push(format!(
                    "Particle budget: {}",
                    c.quality.max_active_particles
                ));
                b.push(format!(
                    "New particles per tick: {}",
                    c.quality.max_spawns_per_tick
                ));
                b.push(format!(
                    "Follow the game's Particles option: {}",
                    on(c.quality.respect_game_particle_setting)
                ));
                b.push(format!(
                    "With Minimal particles: {}",
                    match c.quality.minimal_mode {
                        MinimalMode::NoExtraParticles => "no extra particles",
                        MinimalMode::FeedbackOnly => "hits and slimes only",
                    }
                ));
                b.push(format!("Diagnostics counters: {}", on(c.show_counters)));
                b.push("Back".into());
            }
        }
        b
    }

    pub fn click(&mut self, c: &mut Config, button: usize) -> Outcome {
        let was_confirm = std::mem::take(&mut self.confirm_reset);
        match self.page {
            Page::Main => match button {
                0 => c.enabled = !c.enabled,
                1 => {
                    let p = match c.preset {
                        Preset::Low => Preset::Balanced,
                        Preset::Balanced => Preset::High,
                        Preset::High | Preset::Custom => Preset::Low,
                    };
                    c.apply_preset(p);
                }
                2 => c.quality.global_intensity = next(&AMOUNTS, c.quality.global_intensity),
                3 => {
                    c.quality.emission_distance_blocks =
                        next(&DISTANCES, c.quality.emission_distance_blocks);
                    c.preset = c.detect_preset();
                }
                4 => c.reduced_motion = !c.reduced_motion,
                5 => return self.go(Page::Effects),
                6 => return self.go(Page::Advanced),
                7 => {
                    if was_confirm {
                        *c = Config::default();
                    } else {
                        self.confirm_reset = true;
                        return Outcome::Navigate;
                    }
                }
                _ => return Outcome::Nothing,
            },
            Page::Effects => match Family::ALL.get(button) {
                Some(&f) => return self.go(Page::Effect(f)),
                None if button == Family::ALL.len() => return self.go(Page::Main),
                None => return Outcome::Nothing,
            },
            Page::Effect(f) => {
                let extra: &[&str] = match f {
                    Family::Slime | Family::Amethyst | Family::Armor | Family::Ores => &["x"],
                    Family::Soul => &["a", "s"],
                    Family::Water => &["r", "n", "t"],
                    Family::Hit | Family::Charge => &[],
                };
                let lists = matches!(f, Family::Hit | Family::Armor | Family::Ores);
                let n = 2 + extra.len();
                match button {
                    0 => c.family_mut(f).enabled = !c.family(f).enabled,
                    1 => {
                        let v = next(&AMOUNTS, c.family(f).intensity);
                        c.family_mut(f).intensity = v;
                    }
                    i if i < n => match (f, i - 2) {
                        (Family::Slime, _) => {
                            let at = SLIME_COLORS.iter().position(|(_, v)| *v == c.slime_color);
                            c.slime_color = match at {
                                Some(i) => SLIME_COLORS[(i + 1) % SLIME_COLORS.len()].1,
                                None => SLIME_COLORS[0].1,
                            };
                        }
                        (Family::Amethyst, _) => {
                            c.amethyst_full_clusters_only = !c.amethyst_full_clusters_only
                        }
                        (Family::Armor, _) => {
                            c.armor_show_local_first_person = !c.armor_show_local_first_person
                        }
                        (Family::Ores, _) => c.ores_exposed_faces_only = !c.ores_exposed_faces_only,
                        (Family::Soul, 0) => c.soul_ambient = !c.soul_ambient,
                        (Family::Soul, _) => c.soul_sprinting = !c.soul_sprinting,
                        (Family::Water, 0) => {
                            c.water_radius = next(&RADII, c.water_radius);
                            c.preset = c.detect_preset();
                        }
                        (Family::Water, 1) => {
                            c.water_samples = next(&SAMPLES, c.water_samples);
                            c.preset = c.detect_preset();
                        }
                        (Family::Water, _) => c.water_biome_tint = !c.water_biome_tint,
                        _ => return Outcome::Nothing,
                    },
                    i if lists && i == n => return Outcome::Nothing,
                    i if lists && i == n + 1 => return Outcome::Reload,
                    i if i == n + if lists { 2 } else { 0 } => return self.go(Page::Effects),
                    _ => return Outcome::Nothing,
                }
            }
            Page::Advanced => match button {
                0 => {
                    c.quality.max_active_particles = next(&BUDGETS, c.quality.max_active_particles);
                    c.preset = c.detect_preset();
                }
                1 => {
                    c.quality.max_spawns_per_tick = next(&SPAWNS, c.quality.max_spawns_per_tick);
                    c.preset = c.detect_preset();
                }
                2 => {
                    c.quality.respect_game_particle_setting =
                        !c.quality.respect_game_particle_setting
                }
                3 => {
                    c.quality.minimal_mode = match c.quality.minimal_mode {
                        MinimalMode::NoExtraParticles => MinimalMode::FeedbackOnly,
                        MinimalMode::FeedbackOnly => MinimalMode::NoExtraParticles,
                    }
                }
                4 => c.show_counters = !c.show_counters,
                5 => return self.go(Page::Main),
                _ => return Outcome::Nothing,
            },
        }
        Outcome::Changed
    }

    fn go(&mut self, page: Page) -> Outcome {
        self.page = page;
        Outcome::Navigate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_of_every_page_is_handled() {
        let pages = [Page::Main, Page::Effects, Page::Advanced]
            .into_iter()
            .chain(Family::ALL.map(Page::Effect));
        for page in pages {
            let mut c = Config::default();
            let mut m = Menu {
                page,
                confirm_reset: false,
            };
            let n = m.labels(&c, None).len();
            for i in 0..n {
                m.page = page;
                let out = m.click(&mut c, i);
                let label = &Menu {
                    page,
                    confirm_reset: false,
                }
                .labels(&Config::default(), None)[i];
                if label.starts_with("Lists:") {
                    assert_eq!(out, Outcome::Nothing, "{page:?} {label}");
                } else {
                    assert_ne!(
                        out,
                        Outcome::Nothing,
                        "{page:?} button {i} ({label}) does nothing"
                    );
                }
                c.clamp();
            }
            m.page = page;
            assert_eq!(m.click(&mut c, n + 3), Outcome::Nothing);
        }
    }

    #[test]
    fn preset_cycles_and_shows_custom_after_a_manual_change() {
        let mut c = Config::default();
        let mut m = Menu::default();
        assert!(m.labels(&c, None)[1].ends_with("Low"));
        m.click(&mut c, 1);
        assert!(m.labels(&c, None)[1].ends_with("Balanced"));
        m.page = Page::Advanced;
        m.click(&mut c, 0);
        m.page = Page::Main;
        assert!(m.labels(&c, None)[1].ends_with("Custom"));
    }

    #[test]
    fn reset_needs_two_clicks() {
        let mut c = Config {
            enabled: false,
            ..Config::default()
        };
        let mut m = Menu::default();
        assert_eq!(m.click(&mut c, 7), Outcome::Navigate);
        assert!(!c.enabled);
        assert_eq!(m.click(&mut c, 7), Outcome::Changed);
        assert_eq!(c, Config::default());
    }

    #[test]
    fn family_toggle_and_navigation() {
        let mut c = Config::default();
        let mut m = Menu::default();
        m.click(&mut c, 5);
        assert_eq!(m.page, Page::Effects);
        m.click(&mut c, 7);
        assert_eq!(m.page, Page::Effect(Family::Water));
        m.click(&mut c, 0);
        assert!(!c.family(Family::Water).enabled);
        let back = m.labels(&c, None).len() - 1;
        m.click(&mut c, back);
        assert_eq!(m.page, Page::Effects);
        assert!(m.labels(&c, None)[7].contains("Off"));
    }

    #[test]
    fn labels_are_plain_words() {
        let c = Config::default();
        let m = Menu::default();
        for l in m.labels(&c, None) {
            for bad in ["native_", "hook", "uniform", "stride", "_"] {
                assert!(!l.contains(bad), "{l}");
            }
        }
    }
}
