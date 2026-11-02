// The renderer side of Visuality for Horizon: re-exports the effect engine to
// the game crate and draws the particles once the world is drawn.
// Ordinary comments only: this file is a support module payload.

pub use visuality_horizon_engine as engine;

use engine::engine::Lifecycle;

/// Whether the mod has anything to draw this frame.
pub fn active() -> bool {
    engine::runtime::with_existing(|rt| {
        rt.engine.state() == Lifecycle::ActiveWorld && rt.engine.active_len() > 0
    })
    .unwrap_or(false)
}

/// `native_render_needs`: keep the world's depth while particles are live,
/// so they are hidden behind terrain.
pub fn render_needs(needs: &mut super::RenderNeeds) {
    if active() {
        needs.world_depth = true;
    }
}

/// `native_world_drawn`: one draw call with the frame's particles.
pub fn world_drawn(world: &mut super::WorldDrawn<'_>) {
    if !active() {
        return;
    }
    let camera = [world.camera.x, world.camera.y, world.camera.z];
    let view_proj = world.view_proj.to_cols_array();
    engine::runtime::with_existing(|rt| {
        rt.draw(engine::render::Target {
            device: world.device,
            queue: world.queue,
            encoder: &mut *world.encoder,
            color_view: world.color_view,
            color_format: world.color_format,
            size: world.size,
            depth: world.depth,
            depth_size: world.depth_size,
            view_proj,
            camera,
        });
    });
}
