use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Instant;

use mod_api::{
    self, ClientHost, ClientTickPostEvent, Hooks, KeyRegistration, Registrar, RegistrationContext,
    SettingSpec,
};

const ID: &str = "gamma";
const KEY_TOGGLE: &str = "toggle";
const SETTING_ENABLED: &str = "enabled";

pub const DEFAULT_NORMAL_GAMMA: f32 = 0.5;
pub const GAMMA_HIGH: f32 = 15.0;
const GAMMA_VANILLA_MAX: f32 = 1.0;

const PENDING_NONE: u8 = 0;
const PENDING_APPLY: u8 = 1;
const PENDING_NOTIFY_HIGH: u8 = 2;
const PENDING_NOTIFY_LOW: u8 = 3;
const PENDING_NOTIFY_HELD: u8 = 4;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static BOOSTED: AtomicBool = AtomicBool::new(false);
static HELD: AtomicBool = AtomicBool::new(false);
static BOOST_APPLIED: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicU8 = AtomicU8::new(PENDING_NONE);

#[derive(Default)]
struct Mod;

fn normal_gamma(settings_gamma: f32) -> f32 {
    if settings_gamma.is_finite() && (0.0..=GAMMA_VANILLA_MAX).contains(&settings_gamma) {
        settings_gamma
    } else {
        DEFAULT_NORMAL_GAMMA
    }
}

fn restore_action(
    settings_gamma: f32,
    renderer_gamma: Option<f32>,
    notice: Option<&'static str>,
    force_check: bool,
) -> Option<GammaAction> {
    let was_applied =
        BOOST_APPLIED.load(Ordering::Relaxed) && BOOST_APPLIED.swap(false, Ordering::Relaxed);
    let over_vanilla = renderer_gamma.is_some_and(|rg| rg > GAMMA_VANILLA_MAX);
    let target = normal_gamma(settings_gamma);
    let needs_restore = (was_applied || over_vanilla || force_check)
        && renderer_gamma.is_some_and(|rg| rg != target);

    if needs_restore || notice.is_some() {
        Some(GammaAction {
            gamma: needs_restore.then_some(target),
            notice,
        })
    } else {
        None
    }
}

/// Updates the target boost state (`1500%` or normal) and schedules a one-shot apply/toast.
pub fn set_boosted(enabled: bool, notify: bool) {
    BOOSTED.store(enabled, Ordering::Relaxed);
    let pending = match (enabled, notify) {
        (true, true) => PENDING_NOTIFY_HIGH,
        (false, true) => PENDING_NOTIFY_LOW,
        (_, false) => PENDING_APPLY,
    };
    PENDING.store(pending, Ordering::Relaxed);
}

#[cfg(test)]
fn reset_state_for_tests() {
    INITIALIZED.store(false, Ordering::Relaxed);
    BOOSTED.store(false, Ordering::Relaxed);
    HELD.store(false, Ordering::Relaxed);
    BOOST_APPLIED.store(false, Ordering::Relaxed);
    PENDING.store(PENDING_NONE, Ordering::Relaxed);
}

/// Target renderer gamma / toast update for the render thread, or `None` when idle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GammaAction {
    pub gamma: Option<f32>,
    pub notice: Option<&'static str>,
}

/// Computes the renderer-only gamma update right after Horizon's `hold_fullbright()` check.
///
/// Only `renderer.gamma` is ever changed; `settings.gamma` (`options.txt`) is read-only so the
/// player's configured brightness survives options saves while boosted and mod removal never leaves
/// `15.0` persisted in `options.txt`. Turning the boost off or joining an external server restores
/// `renderer.gamma` to the player's configured `settings_gamma`.
pub fn take_gamma_action(
    fullbright_held: bool,
    settings_gamma: f32,
    renderer_gamma: Option<f32>,
) -> Option<GammaAction> {
    if HELD.load(Ordering::Relaxed) != fullbright_held {
        HELD.store(fullbright_held, Ordering::Relaxed);
    }

    let pending = if PENDING.load(Ordering::Relaxed) != PENDING_NONE {
        PENDING.swap(PENDING_NONE, Ordering::Relaxed)
    } else {
        PENDING_NONE
    };

    if fullbright_held {
        let notice = (pending == PENDING_NOTIFY_HELD)
            .then_some("Gamma: disabled on external servers");
        return restore_action(settings_gamma, renderer_gamma, notice, false);
    }

    if BOOSTED.load(Ordering::Relaxed) {
        if !BOOST_APPLIED.load(Ordering::Relaxed) {
            BOOST_APPLIED.store(true, Ordering::Relaxed);
        }
        let notice = match pending {
            PENDING_NOTIFY_HIGH => Some("Gamma: 1500%"),
            PENDING_NOTIFY_HELD => Some("Gamma: disabled on external servers"),
            _ => None,
        };
        let needs_write = renderer_gamma.is_some_and(|rg| rg != GAMMA_HIGH);
        if needs_write || notice.is_some() {
            return Some(GammaAction {
                gamma: needs_write.then_some(GAMMA_HIGH),
                notice,
            });
        }
        return None;
    }

    let notice = match pending {
        PENDING_NOTIFY_LOW => Some("Gamma: OFF"),
        PENDING_NOTIFY_HELD => Some("Gamma: disabled on external servers"),
        _ => None,
    };
    let force_check = matches!(pending, PENDING_NOTIFY_LOW | PENDING_APPLY);
    restore_action(settings_gamma, renderer_gamma, notice, force_check)
}

/// Applies the pending render-only gamma update (used by unit tests).
pub fn apply_gamma(
    fullbright_held: bool,
    settings_gamma: f32,
    mut r_gamma: Option<&mut f32>,
    message: &mut Option<(String, Instant)>,
) {
    let renderer_val = r_gamma.as_deref().copied();
    let Some(action) = take_gamma_action(fullbright_held, settings_gamma, renderer_val) else {
        return;
    };

    if let Some(gamma) = action.gamma
        && let Some(rg) = r_gamma.as_deref_mut()
        && *rg != gamma
    {
        *rg = gamma;
    }
    if let Some(notice) = action.notice {
        *message = Some((notice.to_owned(), Instant::now()));
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE, "key.keyboard.g").label("key.gamma.toggle"),
        );
        if let Err(error) = r.register_setting(SettingSpec::toggle(
            SETTING_ENABLED,
            "gamma.setting.enabled",
            false,
        )) {
            eprintln!("[{ID}] setting refused: {error:?}");
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        if !INITIALIZED.swap(true, Ordering::Relaxed) {
            let initial = host.setting_bool(SETTING_ENABLED).unwrap_or(false);
            BOOSTED.store(initial, Ordering::Relaxed);
            if initial && !HELD.load(Ordering::Relaxed) {
                PENDING.store(PENDING_APPLY, Ordering::Relaxed);
            }
        }

        let mut enabled = BOOSTED.load(Ordering::Relaxed);
        if let Some(setting) = host.setting_bool(SETTING_ENABLED)
            && setting != enabled
        {
            enabled = setting;
            BOOSTED.store(enabled, Ordering::Relaxed);
            if HELD.load(Ordering::Relaxed) {
                if enabled {
                    PENDING.store(PENDING_NOTIFY_HELD, Ordering::Relaxed);
                }
            } else {
                PENDING.store(PENDING_APPLY, Ordering::Relaxed);
            }
        }

        let mut clicks = 0usize;
        while host.consume_click(ID, KEY_TOGGLE) {
            clicks += 1;
        }

        if clicks > 0 {
            if HELD.load(Ordering::Relaxed) {
                PENDING.store(PENDING_NOTIFY_HELD, Ordering::Relaxed);
            } else if clicks % 2 == 1 {
                enabled = !enabled;
                set_boosted(enabled, true);
                host.set_setting(
                    ID,
                    SETTING_ENABLED,
                    if enabled { "true" } else { "false" },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_state_for_tests();
        guard
    }

    #[test]
    fn toggles_boost_and_restores_player_brightness() {
        let _guard = lock_and_reset();
        let settings_gamma = 0.75_f32;
        let mut r_gamma = 0.75_f32;
        let mut message = None;

        // Idle when disabled does not alter vanilla brightness.
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, 0.75);
        assert!(message.is_none());

        // Enabling boosts only renderer.gamma; settings.gamma stays 0.75.
        set_boosted(true, true);
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, GAMMA_HIGH);
        assert_eq!(
            message.as_ref().map(|(t, _)| t.as_str()),
            Some("Gamma: 1500%")
        );

        message = None;
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert!(message.is_none());
        assert_eq!(r_gamma, GAMMA_HIGH);

        // Disabling restores the player's own 0.75 brightness, not 1.0.
        set_boosted(false, true);
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, 0.75);
        assert_eq!(message.as_ref().map(|(t, _)| t.as_str()), Some("Gamma: OFF"));
    }

    #[test]
    fn non_default_brightness_survives_options_save_while_boosted() {
        let _guard = lock_and_reset();
        let settings_gamma = 0.35_f32;
        let mut r_gamma = 0.35_f32;
        let mut message = None;

        set_boosted(true, true);
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, GAMMA_HIGH);

        // Simulate Horizon saving options.txt and calling apply_ui_settings() during boost.
        let saved_to_options_txt = settings_gamma;
        assert_eq!(saved_to_options_txt, 0.35);
        r_gamma = settings_gamma;

        // Next visual tick reapplies 15.0 to renderer only; saved setting stays 0.35.
        message = None;
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, GAMMA_HIGH);
        assert_eq!(saved_to_options_txt, 0.35);
        assert!(message.is_none());
    }

    #[test]
    fn respects_external_server_fullbright_restriction() {
        let _guard = lock_and_reset();
        let mut settings_gamma = 0.65_f32;
        let mut r_gamma = 0.65_f32;
        let mut message = None;

        set_boosted(true, true);
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, GAMMA_HIGH);

        // Joining external server restores player's 0.65 brightness immediately.
        message = None;
        apply_gamma(true, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, 0.65);
        assert!(message.is_none());

        // Adjusting vanilla brightness slider on external server stays intact.
        settings_gamma = 0.25;
        r_gamma = 0.25;
        apply_gamma(true, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, 0.25);

        // Toggle attempt on external server leaves 0.25 intact and shows toast.
        PENDING.store(PENDING_NOTIFY_HELD, Ordering::Relaxed);
        apply_gamma(true, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, 0.25);
        assert_eq!(
            message.as_ref().map(|(t, _)| t.as_str()),
            Some("Gamma: disabled on external servers")
        );

        // Returning to singleplayer restores the active boost on renderer only.
        message = None;
        apply_gamma(false, settings_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!(r_gamma, GAMMA_HIGH);
        assert_eq!(settings_gamma, 0.25);
    }
}
