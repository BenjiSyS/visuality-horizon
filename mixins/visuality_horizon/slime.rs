// A normal slime landing on the client: Visuality's blobs replace the
// item_slime crumbs when the Slime blobs effect is on. Magma and sulfur cubes,
// and every other tick, take the original path. Only the landing flag the
// original would set is set here; gameplay state is untouched.
fn visuality_slime(m: &mut Mob) -> core::ops::ControlFlow<()> {
    use horizon_client_render::gfx::native_render::visuality_horizon::engine as vh;
    if m.species() != Species::Slime || !(m.on_ground && !m.extra.flag2) {
        return core::ops::ControlFlow::Continue(());
    }
    let feet = [m.position.x, m.position.y, m.position.z];
    let (id, size, width) = (m.id as i32, m.extra.size, m.width() as f32);
    let replaced = vh::runtime::with_existing(|rt| rt.engine.slime_landed(id, feet, size, width))
        .unwrap_or(false);
    if replaced {
        m.extra.flag2 = m.on_ground;
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
