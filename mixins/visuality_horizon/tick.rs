// Once a client tick: Visuality's emitters and its particles' fixed step.
fn visuality_tick(game: &mut crate::game::Game<'_>) {
    crate::game::visuality_horizon::tick(game);
}
