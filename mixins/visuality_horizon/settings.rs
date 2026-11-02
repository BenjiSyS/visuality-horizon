// Visuality's settings page.
fn visuality_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    crate::game::visuality_horizon::page(game, mod_id, buttons);
}
