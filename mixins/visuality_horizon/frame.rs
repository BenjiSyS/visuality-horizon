// Once a frame: Visuality's interpolated draw list.
fn visuality_frame(game: &mut crate::game::Game<'_>) {
    crate::game::visuality_horizon::frame(game);
}
