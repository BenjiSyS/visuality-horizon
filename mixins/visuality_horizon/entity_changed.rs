// An entity id was added or removed: Visuality forgets its cooldown.
fn visuality_entity_changed(id: i32) {
    crate::game::visuality_horizon::entity_changed(id);
}
