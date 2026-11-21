use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
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

const PENDING_NONE: u8 = 0;
const PENDING_SET_LOW: u8 = 1;
const PENDING_NOTIFY_HIGH: u8 = 2;
const PENDING_NOTIFY_LOW: u8 = 3;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static BOOSTED: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicU8 = AtomicU8::new(PENDING_NONE);

#[derive(Default)]
struct Mod;

/// Updates the target gamma state (`100%` or `1500%`) and schedules a one-shot apply/toast.
pub fn set_boosted(enabled: bool, notify: bool) {
    BOOSTED.store(enabled, Ordering::Relaxed);
    let pending = match (enabled, notify) {
        (true, true) => PENDING_NOTIFY_HIGH,
        (true, false) => PENDING_NONE,
        (false, true) => PENDING_NOTIFY_LOW,
        (false, false) => PENDING_SET_LOW,
    };
    PENDING.store(pending, Ordering::Relaxed);
}

/// Applies the active gamma preset (`1.0` = `100%` or `15.0` = `1500%`) on the render thread.
/// Idle `100%` frames only perform two relaxed atomic loads with zero allocations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GammaAction {
    pub gamma: f32,
    pub notice: Option<&'static str>,
}

/// Returns the target gamma update for the current tick, or `None` when idle.
pub fn take_gamma_action() -> Option<GammaAction> {
    let boosted = BOOSTED.load(Ordering::Relaxed);
    let pending = if PENDING.load(Ordering::Relaxed) != PENDING_NONE {
        PENDING.swap(PENDING_NONE, Ordering::Relaxed)
    } else {
        PENDING_NONE
    };

    let gamma = if boosted {
        GAMMA_HIGH
    } else if pending != PENDING_NONE {
        GAMMA_LOW
    } else {
        return None;
    };

    let notice = match pending {
        PENDING_NOTIFY_HIGH => Some("Gamma: 1500%"),
        PENDING_NOTIFY_LOW => Some("Gamma: 100%"),
        _ => None,
    };

    Some(GammaAction { gamma, notice })
}

/// Applies the pending gamma update to explicit fields (used by tests).
pub fn apply_gamma(
    s_gamma: &mut f32,
    r_gamma: Option<&mut f32>,
    message: &mut Option<(String, Instant)>,
) {
    let Some(action) = take_gamma_action() else {
        return;
    };

    if *s_gamma != action.gamma {
        *s_gamma = action.gamma;
    }
    if let Some(rg) = r_gamma
        && *rg != action.gamma
    {
        *rg = action.gamma;
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
            set_boosted(initial, false);
        }

        let mut enabled = BOOSTED.load(Ordering::Relaxed);
        if let Some(setting) = host.setting_bool(SETTING_ENABLED)
            && setting != enabled
        {
            enabled = setting;
            set_boosted(enabled, false);
        }

        let mut toggled = false;
        while host.consume_click(ID, KEY_TOGGLE) {
            enabled = !enabled;
            toggled = true;
        }

        if toggled {
            set_boosted(enabled, true);
            host.set_setting(
                ID,
                SETTING_ENABLED,
                if enabled { "true" } else { "false" },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_between_100_and_1500_percent() {
        let mut s_gamma = 0.5_f32;
        let mut r_gamma = 0.5_f32;
        let mut message = None;

        set_boosted(false, false);
        apply_gamma(&mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (GAMMA_LOW, GAMMA_LOW));
        assert!(message.is_none());

        set_boosted(true, true);
        apply_gamma(&mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (GAMMA_HIGH, GAMMA_HIGH));
        assert_eq!(message.as_ref().map(|(t, _)| t.as_str()), Some("Gamma: 1500%"));

        message = None;
        apply_gamma(&mut s_gamma, Some(&mut r_gamma), &mut message);
        assert!(message.is_none());

        set_boosted(false, true);
        apply_gamma(&mut s_gamma, Some(&mut r_gamma), &mut message);
        assert_eq!((s_gamma, r_gamma), (GAMMA_LOW, GAMMA_LOW));
        assert_eq!(message.as_ref().map(|(t, _)| t.as_str()), Some("Gamma: 100%"));
    }
}
