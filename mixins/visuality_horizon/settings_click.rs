// A click on Visuality's settings page.
fn visuality_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    crate::game::visuality_horizon::click(game, mod_id, button);
}
