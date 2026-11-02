// After the world is drawn: Visuality's particles over it, depth-tested.
fn visuality_world_drawn(world: &mut WorldDrawn<'_>) {
    visuality_horizon::world_drawn(world);
}
