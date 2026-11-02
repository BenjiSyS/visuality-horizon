// Visuality's row in Options > Mods.
fn visuality_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    crate::game::visuality_horizon::listed(mods);
}
