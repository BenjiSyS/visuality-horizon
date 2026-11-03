//! The packaged atlas and its manifest: validated, then decoded once.
//!
//! The atlas PNG and `atlas.json` are compiled into the game from this
//! crate's `data/` folder; nothing is read from disk or the network at run
//! time.

use serde::Deserialize;

/// The packed atlas and its manifest, compiled in from `atlas_data.rs`
/// (generated from `data/` by tools/atlas_rs.py).
pub const ATLAS_PNG: &[u8] = crate::atlas_data::ATLAS_PNG;
pub const ATLAS_JSON: &[u8] = crate::atlas_data::ATLAS_JSON.as_bytes();

pub const MAX_ATLAS_SIDE: u32 = 1024;
pub const MAX_FRAMES: usize = 512;
pub const MAX_FRAMES_PER_ANIMATION: usize = 32;

/// Every animation the effects use, by manifest id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Anim {
    BoneA,
    BoneB,
    WitherBoneA,
    WitherBoneB,
    Feather,
    FeatherWarm,
    FeatherCold,
    Emerald,
    SlimeSmall,
    SlimeMedium,
    SlimeLarge,
    Sparkle,
    SparkleAmethyst,
    ChargeA,
    ChargeB,
    ChargeC,
    SoulAmbient,
    SoulSprint,
    WaterRing,
}

impl Anim {
    pub const ALL: [Anim; 19] = [
        Anim::BoneA,
        Anim::BoneB,
        Anim::WitherBoneA,
        Anim::WitherBoneB,
        Anim::Feather,
        Anim::FeatherWarm,
        Anim::FeatherCold,
        Anim::Emerald,
        Anim::SlimeSmall,
        Anim::SlimeMedium,
        Anim::SlimeLarge,
        Anim::Sparkle,
        Anim::SparkleAmethyst,
        Anim::ChargeA,
        Anim::ChargeB,
        Anim::ChargeC,
        Anim::SoulAmbient,
        Anim::SoulSprint,
        Anim::WaterRing,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Anim::BoneA => "bone_a",
            Anim::BoneB => "bone_b",
            Anim::WitherBoneA => "wither_bone_a",
            Anim::WitherBoneB => "wither_bone_b",
            Anim::Feather => "feather",
            Anim::FeatherWarm => "feather_warm",
            Anim::FeatherCold => "feather_cold",
            Anim::Emerald => "emerald",
            Anim::SlimeSmall => "slime_small",
            Anim::SlimeMedium => "slime_medium",
            Anim::SlimeLarge => "slime_large",
            Anim::Sparkle => "sparkle",
            Anim::SparkleAmethyst => "sparkle_amethyst",
            Anim::ChargeA => "charge_a",
            Anim::ChargeB => "charge_b",
            Anim::ChargeC => "charge_c",
            Anim::SoulAmbient => "soul_ambient",
            Anim::SoulSprint => "soul_sprint",
            Anim::WaterRing => "water_ring",
        }
    }
    pub fn orientation(self) -> Orientation {
        match self {
            Anim::WaterRing => Orientation::Surface,
            _ => Orientation::Billboard,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Billboard,
    Surface,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    schema_version: u32,
    atlas: AtlasDecl,
    animations: Vec<AnimDecl>,
    #[serde(default)]
    provenance: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AtlasDecl {
    path: String,
    width: u32,
    height: u32,
    color_space: String,
    alpha: String,
    sampling: String,
    #[serde(default)]
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimDecl {
    id: String,
    orientation: String,
    #[serde(rename = "loop")]
    looping: bool,
    pivot_pixels: [u32; 2],
    frames: Vec<FrameDecl>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameDecl {
    rect: [u32; 4],
    duration_ms: u32,
    #[serde(default)]
    source: String,
}

/// One frame: its UV rectangle (u0, v0, u1, v1) and the end of its interval
/// on the normalized 0..1 phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub uv: [f32; 4],
    pub phase_end: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub frames: Vec<Frame>,
    pub looping: bool,
}

impl Animation {
    /// The frame for `phase` (age / lifetime): the first cumulative interval
    /// containing it, clamped to the last frame; looping wraps.
    pub fn frame(&self, phase: f32) -> &Frame {
        let mut p = if phase.is_finite() { phase } else { 0.0 };
        if self.looping {
            p = p.rem_euclid(1.0);
        }
        let p = p.clamp(0.0, 1.0);
        self.frames
            .iter()
            .find(|f| p < f.phase_end)
            .unwrap_or_else(|| self.frames.last().expect("validated non-empty"))
    }
}

pub struct Atlas {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha sRGB RGBA8.
    pub rgba: Vec<u8>,
    pub animations: Vec<Animation>,
}

impl Atlas {
    pub fn animation(&self, a: Anim) -> &Animation {
        &self.animations[a as usize]
    }
}

/// Validates the manifest and decodes the PNG within fixed bounds.
pub fn load(json: &[u8], png_bytes: &[u8]) -> Result<Atlas, String> {
    let m: ManifestFile =
        serde_json::from_slice(json).map_err(|e| format!("atlas manifest: {e}"))?;
    if m.schema_version != 1 {
        return Err(format!(
            "atlas manifest: schema {} unsupported",
            m.schema_version
        ));
    }
    let a = &m.atlas;
    if a.path.contains("..") || a.path.contains(':') || a.path.starts_with('/') {
        return Err("atlas manifest: path must be a plain relative name".into());
    }
    if a.color_space != "srgb" || a.alpha != "straight" || a.sampling != "nearest" {
        return Err("atlas manifest: needs srgb, straight alpha, nearest sampling".into());
    }
    if a.width == 0 || a.height == 0 || a.width > MAX_ATLAS_SIDE || a.height > MAX_ATLAS_SIDE {
        return Err(format!(
            "atlas manifest: size {}x{} out of range",
            a.width, a.height
        ));
    }
    let _ = (&m.provenance, &a.sha256);
    let mut animations: Vec<Option<Animation>> = vec![None; Anim::ALL.len()];
    let mut total = 0;
    for decl in &m.animations {
        let Some(anim) = Anim::ALL.iter().find(|x| x.id() == decl.id) else {
            return Err(format!("atlas manifest: unknown animation {}", decl.id));
        };
        if animations[*anim as usize].is_some() {
            return Err(format!("atlas manifest: duplicate animation {}", decl.id));
        }
        let want = match anim.orientation() {
            Orientation::Billboard => "billboard",
            Orientation::Surface => "surface",
        };
        if decl.orientation != want {
            return Err(format!("atlas manifest: {} must be {want}", decl.id));
        }
        if decl.frames.is_empty() || decl.frames.len() > MAX_FRAMES_PER_ANIMATION {
            return Err(format!("atlas manifest: {} frame count", decl.id));
        }
        total += decl.frames.len();
        if total > MAX_FRAMES {
            return Err("atlas manifest: too many frames".into());
        }
        let sum: u64 = decl.frames.iter().map(|f| f.duration_ms as u64).sum();
        let mut acc = 0u64;
        let mut frames = Vec::with_capacity(decl.frames.len());
        let mut cell = None;
        for f in &decl.frames {
            let [x, y, w, h] = f.rect;
            if w == 0
                || h == 0
                || x.checked_add(w).is_none_or(|r| r > a.width)
                || y.checked_add(h).is_none_or(|b| b > a.height)
            {
                return Err(format!(
                    "atlas manifest: {} rect {:?} outside atlas",
                    decl.id, f.rect
                ));
            }
            if f.duration_ms == 0 || f.duration_ms > 10_000 {
                return Err(format!("atlas manifest: {} frame duration", decl.id));
            }
            if *cell.get_or_insert((w, h)) != (w, h) {
                return Err(format!("atlas manifest: {} frames differ in size", decl.id));
            }
            let _ = &f.source;
            acc += f.duration_ms as u64;
            frames.push(Frame {
                uv: [
                    x as f32 / a.width as f32,
                    y as f32 / a.height as f32,
                    (x + w) as f32 / a.width as f32,
                    (y + h) as f32 / a.height as f32,
                ],
                phase_end: acc as f32 / sum as f32,
            });
        }
        let (w, h) = cell.expect("non-empty");
        if decl.pivot_pixels[0] > w || decl.pivot_pixels[1] > h {
            return Err(format!(
                "atlas manifest: {} pivot outside its cell",
                decl.id
            ));
        }
        if let Some(last) = frames.last_mut() {
            last.phase_end = 1.0;
        }
        animations[*anim as usize] = Some(Animation {
            frames,
            looping: decl.looping,
        });
    }
    let animations = animations
        .into_iter()
        .zip(Anim::ALL)
        .map(|(a, id)| a.ok_or_else(|| format!("atlas manifest: missing animation {}", id.id())))
        .collect::<Result<Vec<_>, _>>()?;
    let rgba = decode_png(png_bytes, a.width, a.height)?;
    Ok(Atlas {
        width: a.width,
        height: a.height,
        rgba,
        animations,
    })
}

fn decode_png(bytes: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let limits = png::Limits {
        bytes: (MAX_ATLAS_SIDE * MAX_ATLAS_SIDE * 4) as usize,
    };
    let mut decoder = png::Decoder::new_with_limits(std::io::Cursor::new(bytes), limits);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("atlas png: {e}"))?;
    let info = reader.info();
    if info.width != width || info.height != height {
        return Err(format!(
            "atlas png: {}x{} but manifest says {width}x{height}",
            info.width, info.height
        ));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "atlas png: size overflow".to_string())?;
    let mut buf = vec![0; size];
    let out = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("atlas png: {e}"))?;
    buf.truncate(out.buffer_size());
    if out.color_type != png::ColorType::Rgba || out.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "atlas png: expected RGBA8, got {:?} {:?}",
            out.color_type, out.bit_depth
        ));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_rgba(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut wr = e.write_header().unwrap();
        wr.write_image_data(&vec![0u8; (w * h * 4) as usize])
            .unwrap();
        wr.finish().unwrap();
        out
    }

    fn manifest(extra: &str, skip: Option<&str>) -> String {
        let anims: Vec<String> = Anim::ALL
            .iter()
            .filter(|a| Some(a.id()) != skip)
            .map(|a| {
                let o = if a.orientation() == Orientation::Surface { "surface" } else { "billboard" };
                format!(
                    r#"{{"id":"{}","orientation":"{o}","loop":false,"pivot_pixels":[8,8],
                    "frames":[{{"rect":[0,0,16,16],"duration_ms":30}},{{"rect":[16,0,16,16],"duration_ms":10}}]}}"#,
                    a.id()
                )
            })
            .collect();
        format!(
            r#"{{"schema_version":1,"atlas":{{"path":"atlas.png","width":64,"height":32,
            "color_space":"srgb","alpha":"straight","sampling":"nearest"}},"animations":[{}{extra}]}}"#,
            anims.join(",")
        )
    }

    #[test]
    fn valid_manifest_loads_and_weights_frames() {
        let a = load(manifest("", None).as_bytes(), &png_rgba(64, 32)).unwrap();
        let anim = a.animation(Anim::WaterRing);
        assert_eq!(anim.frames.len(), 2);
        assert!((anim.frames[0].phase_end - 0.75).abs() < 1e-6);
        assert_eq!(anim.frame(0.5).uv[0], 0.0);
        assert_eq!(anim.frame(0.8).uv[0], 0.25);
        // Past the end clamps to the last frame instead of wrapping.
        assert_eq!(anim.frame(3.0).uv[0], 0.25);
        assert_eq!(anim.frame(f32::NAN).uv[0], 0.0);
    }

    #[test]
    fn generated_atlas_is_the_packed_files_byte_for_byte() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let png = std::fs::read(dir.join("atlas.png")).expect("data/atlas.png in the source tree");
        let json = std::fs::read_to_string(dir.join("atlas.json")).expect("data/atlas.json");
        assert_eq!(ATLAS_PNG, &png[..], "rerun tools/atlas_rs.py");
        assert_eq!(ATLAS_JSON, json.as_bytes(), "rerun tools/atlas_rs.py");
    }

    #[test]
    fn the_shipped_atlas_is_valid() {
        let a = load(ATLAS_JSON, ATLAS_PNG).expect("shipped atlas");
        assert!(a.width <= MAX_ATLAS_SIDE && a.height <= MAX_ATLAS_SIDE);
        assert_eq!(a.rgba.len(), (a.width * a.height * 4) as usize);
        // Every animation has visible pixels in each frame.
        for anim in Anim::ALL {
            for f in &a.animation(anim).frames {
                let x0 = (f.uv[0] * a.width as f32) as u32;
                let y0 = (f.uv[1] * a.height as f32) as u32;
                let x1 = (f.uv[2] * a.width as f32) as u32;
                let y1 = (f.uv[3] * a.height as f32) as u32;
                let mut opaque = 0;
                for y in y0..y1 {
                    for x in x0..x1 {
                        if a.rgba[((y * a.width + x) * 4 + 3) as usize] > 0 {
                            opaque += 1;
                        }
                    }
                }
                assert!(opaque > 0, "{} has an empty frame", anim.id());
            }
        }
    }

    #[test]
    fn invalid_manifests_are_refused() {
        let png = png_rgba(64, 32);
        assert!(load(manifest("", Some("emerald")).as_bytes(), &png).is_err());
        // The first animation again: a duplicate id.
        let full = manifest("", None);
        let first = full
            .split("\"animations\":[")
            .nth(1)
            .unwrap()
            .split("},{")
            .next()
            .unwrap();
        let dup = format!(",{first}}}");
        assert!(load(manifest(&dup, None).as_bytes(), &png).is_err());
        let outside = manifest("", None).replace("[16,0,16,16]", "[60,0,16,16]");
        assert!(load(outside.as_bytes(), &png).is_err());
        let zero = manifest("", None).replace("\"duration_ms\":10", "\"duration_ms\":0");
        assert!(load(zero.as_bytes(), &png).is_err());
        assert!(load(manifest("", None).as_bytes(), &png_rgba(32, 32)).is_err());
        assert!(load(manifest("", None).as_bytes(), b"not a png").is_err());
        assert!(load(b"{", &png).is_err());
        let traversal = manifest("", None).replace("\"atlas.png\"", "\"../x.png\"");
        assert!(load(traversal.as_bytes(), &png).is_err());
    }
}
