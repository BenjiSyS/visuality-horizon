//! Visuality for Horizon: small cosmetic particles for the Horizon client,
//! an original Rust implementation inspired by PinkGoosik's Visuality (MIT).
//!
//! Everything here runs on the window's client only and changes nothing in
//! the world, the server or the network: it reads what the client already
//! shows and draws its own sprites over the world.

pub mod assets;
pub mod config;
pub mod diagnostics;
pub mod engine;
pub mod particles;
#[cfg(feature = "gpu")]
pub mod render;
pub mod rng;
pub mod runtime;
pub mod settings;

pub use engine::MOD_ID;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TITLE: &str = "Visuality";
pub const DESCRIPTION: &str = "Small pixel particles: hit fragments, slime blobs, charged creeper sparks, \
amethyst, armor and ore glints, soul wisps and rain ripples. Client-side only.";
