// A hurt packet: the damage event is the canonical client-visible hit, the
// hurt animation only a second signal of it. Nothing about the packet changes.
fn visuality_hurt(packet: &C) {
    match packet {
        C::DamageEvent(p) => crate::game::visuality_horizon::hurt(
            p.id,
            true,
            p.cause.is_some() || p.direct.is_some(),
        ),
        C::HurtAnimation(p) => crate::game::visuality_horizon::hurt(p.id, false, true),
        _ => {}
    }
}
