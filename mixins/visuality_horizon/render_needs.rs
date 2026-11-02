// Visuality needs the world's depth while it has particles to draw.
fn visuality_render_needs(needs: &mut RenderNeeds) {
    visuality_horizon::render_needs(needs);
}
