// One block the client's ambient pass visited: amethyst, ore or soul sources.
// Reads the loaded world only; the original pass runs unchanged after it.
fn visuality_ambient(ctx: &AnimateContext<'_>, state: BlockStateId, pos: [i32; 3]) {
    use horizon_client_render::gfx::native_render::visuality_horizon::engine as vh;
    use vh::engine::class;
    vh::runtime::with_existing(|rt| {
        let found = rt.engine.block_class(state.0, || ctx.facts.get(state).name.clone());
        if found == class::NONE {
            return;
        }
        let mut exposed = 0u8;
        if found & class::ORE != 0 {
            let faces = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
            for (i, d) in faces.iter().enumerate() {
                let n = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
                // An unloaded neighbour counts as covering the face.
                if !ctx.world.loaded(n[0], n[2]) {
                    continue;
                }
                if !ctx.facts.get(ctx.world.block(n[0], n[1], n[2])).solid_render {
                    exposed |= 1 << i;
                }
            }
        }
        let mut above_air = false;
        if found & class::SOUL != 0 {
            let above = ctx.world.block(pos[0], pos[1] + 1, pos[2]);
            above_air = ctx.world.is_air(above) || ctx.facts.get(above).name.ends_with(":cave_air");
        }
        rt.engine.block_sample(found, pos, exposed, above_air);
    });
}
