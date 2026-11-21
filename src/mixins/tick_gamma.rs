fn tick_gamma(game: &mut Game<'_>) {
    let Some(action) = mod_gamma::take_gamma_action() else {
        return;
    };
    let s_gamma = &mut game.__horizon_mixins_gamma_settings().gamma;
    if *s_gamma != action.gamma {
        *s_gamma = action.gamma;
    }
    if let Some(renderer) = game.__horizon_mixins_gamma_renderer().as_mut()
        && renderer.gamma != action.gamma
    {
        renderer.gamma = action.gamma;
    }
    if let Some(notice) = action.notice {
        *game.__horizon_mixins_gamma_message() =
            Some((notice.to_owned(), ::std::time::Instant::now()));
    }
}
