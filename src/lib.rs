use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::time::Instant;

use mod_api::{
    self, ClientHost, ClientTickPostEvent, Hooks, KeyRegistration, Registrar, RegistrationContext,
    SettingSpec,
};

const ID: &str = "gamma";
const KEY_TOGGLE: &str = "toggle";
const SETTING_ENABLED: &str = "enabled";

pub const GAMMA_LOW: f32 = 1.0;
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
static SAVED_NORMAL_BITS: AtomicU32 = AtomicU32::new(GAMMA_LOW.to_bits());
static PENDING: AtomicU8 = AtomicU8::new(PENDING_NONE);

#[derive(Default)]
struct Mod;

fn saved_normal_gamma() -> f32 {
    let g = f32::from_bits(SAVED_NORMAL_BITS.load(Ordering::Relaxed));
    if g.is_finite() {
        g.clamp(0.0, GAMMA_VANILLA_MAX)
    } else {
        GAMMA_LOW
    }
}

fn remember_normal_gamma(current: f32) {
    if current.is_finite() && (0.0..=GAMMA_VANILLA_MAX).contains(&current) {
        SAVED_NORMAL_BITS.store(current.to_bits(), Ordering::Relaxed);
    }
}

/// Updates the target gamma state (`100%` or `1500%`) and schedules a one-shot apply/toast.
pub fn set_boosted(enabled: bool, notify: bool) {
    BOOSTED.store(enabled, Ordering::Relaxed);
    let pending = match (enabled, notify) {
        (true, true) => PENDING_NOTIFY_HIGH,
        (true, false) => PENDING_APPLY,
        (false, true) => PENDING_NOTIFY_LOW,
        (false, false) => PENDING_APPLY,
    };
    PENDING.store(pending, Ordering::Relaxed);
}

/// Resets internal state for tests or reinitialization.
#[cfg(test)]
fn reset_state_for_tests(normal_gamma: f32) {
    INITIALIZED.store(false, Ordering::Relaxed);
    BOOSTED.store(false, Ordering::Relaxed);
    HELD.store(false, Ordering::Relaxed);
    BOOST_APPLIED.store(false, Ordering::Relaxed);
    SAVED_NORMAL_BITS.store(normal_gamma.clamp(0.0, GAMMA_VANILLA_MAX).to_bits(), Ordering::Relaxed);
    PENDING.store(PENDING_NONE, Ordering::Relaxed);
}

/// Target gamma / toast update for the render thread, or `None` when idle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GammaAction {
    pub gamma: Option<f32>,
    pub notice: Option<&'static str>,
}

/// Computes the gamma update right after Horizon's `hold_fullbright()` check.
///
/// When `fullbright_held` is `true` (external servers / another player's LAN world), boosted gamma
/// is never applied: any previously boosted value is restored to the pre-boost normal brightness,
/// and vanilla brightness values (`0.0..=1.0`) are left completely untouched.
pub fn take_gamma_action(
    fullbright_held: bool,
    current_gamma: f32,
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
        let was_applied = BOOST_APPLIED.swap(false, Ordering::Relaxed);
        let over_vanilla = current_gamma > GAMMA_VANILLA_MAX
            || renderer_gamma.is_some_and(|rg| rg > GAMMA_VANILLA_MAX);

        if was_applied || over_vanilla {
            return Some(GammaAction {
                gamma: Some(saved_normal_gamma()),
                notice,
            });
        }

        return notice.map(|n| GammaAction { gamma: None, notice: Some(n) });
    }

    let boosted = BOOSTED.load(Ordering::Relaxed);
    if boosted {
        if !BOOST_APPLIED.swap(true, Ordering::Relaxed) {
            remember_normal_gamma(current_gamma);
        }
        let notice = match pending {
            PENDING_NOTIFY_HIGH => Some("Gamma: 1500%"),
            PENDING_NOTIFY_HELD => Some("Gamma: disabled on external servers"),
            _ => None,
        };
        let needs_write = current_gamma != GAMMA_HIGH
            || renderer_gamma.is_some_and(|rg| rg != GAMMA_HIGH);
        if needs_write || notice.is_some() {
            return Some(GammaAction {
                gamma: needs_write.then_some(GAMMA_HIGH),
                notice,
            });
        }
        return None;
    }

    BOOST_APPLIED.store(false, Ordering::Relaxed);
    match pending {
        PENDING_NOTIFY_LOW => Some(GammaAction {
            gamma: Some(GAMMA_LOW),
            notice: Some("Gamma: 100%"),
        }),
        PENDING_APPLY => Some(GammaAction {
            gamma: Some(GAMMA_LOW),
            notice: None,
        }),
        PENDING_NOTIFY_HELD => Some(GammaAction {
            gamma: None,
            notice: Some("Gamma: disabled on external servers"),
        }),
        _ => None,
    }
}

/// Applies the pending gamma update to explicit fields (used by tests).
pub fn apply_gamma(
    fullbright_held: bool,
    s_gamma: &mut f32,
    mut r_gamma: Option<&mut f32>,
    message: &mut Option<(String, Instant)>,
) {
    let renderer_val = r_gamma.as_deref().copied();
    let Some(action) = take_gamma_action(fullbright_held, *s_gamma, renderer_val) else {
        return;
    };

    if let Some(gamma) = action.gamma {
        if *s_gamma != gamma {
            *s_gamma = gamma;
        }
        if let Some(rg) = r_gamma.as_deref_mut()
            && *rg != gamma
        {
            *rg = gamma;
        }
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

    fn lock_and_reset(normal_gamma: f32) -> MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_state_for_tests(normal_gamma);
        guard
    }

    #[test]
    fn toggles_between_100_and_1500_percent() {
        let _guard = lock_and_reset(0.5);
        let mut s_gamma = 0.5_f32;
        let mut r_gamma = 0.5_f32;
        let mut message = None;

        // Idle when disabled does not alter vanilla brightness.
        apply_gamma(false, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (0.5, 0.5));
        assert!(message.is_none());

        set_boosted(true, true);
        apply_gamma(false, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (GAMMA_HIGH, GAMMA_HIGH));
        assert_eq!(message.as_ref().map(|(t, _)| t.as_str()), Some("Gamma: 1500%"));

        message = None;
        apply_gamma(false, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert!(message.is_none());
        assert_eq!((s_gamma, r_gamma), (GAMMA_HIGH, GAMMA_HIGH));

        set_boosted(false, true);
        apply_gamma(false, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (GAMMA_LOW, GAMMA_LOW));
        assert_eq!(message.as_ref().map(|(t, _)| t.as_str()), Some("Gamma: 100%"));
    }

    #[test]
    fn respects_external_server_fullbright_restriction() {
        let _guard = lock_and_reset(0.5);
        let mut s_gamma = 0.5_f32;
        let mut r_gamma = 0.5_f32;
        let mut message = None;

        // Boost in singleplayer; pre-boost normal brightness (0.5) is remembered.
        set_boosted(true, true);
        apply_gamma(false, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (GAMMA_HIGH, GAMMA_HIGH));

        // Joining an external server restores normal brightness (0.5) immediately.
        message = None;
        apply_gamma(true, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (0.5, 0.5));
        assert!(message.is_none());

        // Changing vanilla brightness slider on external server stays intact (no overwrites).
        s_gamma = 0.35;
        r_gamma = 0.35;
        apply_gamma(true, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (0.35, 0.35));

        // Attempting to notify/toggle while on external server leaves 0.35 intact.
        PENDING.store(PENDING_NOTIFY_HELD, Ordering::Relaxed);
        apply_gamma(true, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (0.35, 0.35));
        assert_eq!(
            message.as_ref().map(|(t, _)| t.as_str()),
            Some("Gamma: disabled on external servers")
        );

        // Leaving external server back to singleplayer restores the saved boost.
        message = None;
        apply_gamma(false, &mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (GAMMA_HIGH, GAMMA_HIGH));
    }
}
