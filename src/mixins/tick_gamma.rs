fn tick_gamma(game: &mut Game<'_>) {
    let (fullbright_held, settings_gamma) = {
        let s = game.__horizon_mixins_gamma_settings();
        (s.fullbright_held, s.gamma)
    };
    let renderer_gamma = game
        .__horizon_mixins_gamma_renderer()
        .as_ref()
        .map(|r| r.gamma);
    let Some(action) =
        mod_gamma::take_gamma_action(fullbright_held, settings_gamma, renderer_gamma)
    else {
        return;
    };
    if let Some(gamma) = action.gamma
        && let Some(renderer) = game.__horizon_mixins_gamma_renderer().as_mut()
        && renderer.gamma != gamma
    {
        renderer.gamma = gamma;
    }
    if let Some(notice) = action.notice {
        *game.__horizon_mixins_gamma_message() =
            Some((notice.to_owned(), ::std::time::Instant::now()));
    }
}
