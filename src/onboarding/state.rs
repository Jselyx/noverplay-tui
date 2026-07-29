use std::path::PathBuf;

use crate::onboarding::{SoundCloudAccess, zapret::ZapretPlan};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AccountMode {
    Account,
    #[default]
    Guest,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OnboardingStep {
    #[default]
    Welcome,
    Account,
    Providers,
    Audio,
    CheckingSoundCloud,
    ZapretChoice,
    ZapretPath,
    ZapretReview,
    ZapretManual,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OnboardingCommand {
    None,
    ProbeSoundCloud,
    StartAccountLogin,
    PlanZapret(PathBuf),
    ApplyZapret(Box<ZapretPlan>),
    Finish,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingState {
    pub step: OnboardingStep,
    pub selected: usize,
    pub account_mode: AccountMode,
    pub soundcloud_enabled: bool,
    pub yandex_enabled: bool,
    pub audio_output: Option<String>,
    pub audio_outputs: Vec<Option<String>>,
    pub zapret_path: String,
    pub soundcloud_problem: Option<String>,
    pub zapret_plan: Option<ZapretPlan>,
    pub zapret_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingResult {
    pub account_mode: AccountMode,
    pub soundcloud_enabled: bool,
    pub yandex_enabled: bool,
    pub audio_output: Option<String>,
}

impl OnboardingState {
    pub fn new(audio_output: Option<String>) -> Self {
        Self::with_audio_outputs(audio_output, Vec::new())
    }

    pub fn with_audio_outputs(audio_output: Option<String>, outputs: Vec<String>) -> Self {
        let mut audio_outputs = vec![None];
        for output in outputs {
            let output = output.trim().to_string();
            if !output.is_empty() && !audio_outputs.iter().flatten().any(|known| known == &output) {
                audio_outputs.push(Some(output));
            }
        }
        if let Some(current) = audio_output.clone()
            && !audio_outputs
                .iter()
                .flatten()
                .any(|known| known == &current)
        {
            audio_outputs.push(Some(current));
        }
        Self {
            step: OnboardingStep::Welcome,
            selected: 0,
            account_mode: AccountMode::Guest,
            soundcloud_enabled: true,
            yandex_enabled: true,
            audio_output,
            audio_outputs,
            zapret_path: default_zapret_path(),
            soundcloud_problem: None,
            zapret_plan: None,
            zapret_error: None,
        }
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn select_next(&mut self) {
        self.selected = self
            .selected
            .saturating_add(1)
            .min(self.option_count().saturating_sub(1));
    }

    pub fn toggle(&mut self) {
        if self.step != OnboardingStep::Providers {
            return;
        }
        match self.selected {
            0 => self.soundcloud_enabled = !self.soundcloud_enabled,
            1 => self.yandex_enabled = !self.yandex_enabled,
            _ => {}
        }
    }

    pub fn input(&mut self, value: char) {
        if self.step == OnboardingStep::ZapretPath && !value.is_control() {
            self.zapret_path.push(value);
        }
    }

    pub fn backspace(&mut self) {
        if self.step == OnboardingStep::ZapretPath {
            self.zapret_path.pop();
        }
    }

    pub fn confirm(&mut self) -> OnboardingCommand {
        self.zapret_error = None;
        match self.step {
            OnboardingStep::Welcome => self.go(OnboardingStep::Account),
            OnboardingStep::Account => {
                self.account_mode = if self.selected == 0 {
                    AccountMode::Account
                } else {
                    AccountMode::Guest
                };
                let account = self.account_mode;
                self.go(OnboardingStep::Providers);
                if account == AccountMode::Account {
                    return OnboardingCommand::StartAccountLogin;
                }
            }
            OnboardingStep::Providers if self.selected < 2 => {
                self.toggle();
                return OnboardingCommand::None;
            }
            OnboardingStep::Providers => self.go(OnboardingStep::Audio),
            OnboardingStep::Audio => {
                self.audio_output = self
                    .audio_outputs
                    .get(self.selected)
                    .cloned()
                    .unwrap_or(None);
                if self.soundcloud_enabled {
                    self.go(OnboardingStep::CheckingSoundCloud);
                    return OnboardingCommand::ProbeSoundCloud;
                }
                return self.finish();
            }
            OnboardingStep::CheckingSoundCloud => return OnboardingCommand::None,
            OnboardingStep::ZapretChoice => match self.selected {
                0 => self.go(OnboardingStep::ZapretPath),
                1 => self.go(OnboardingStep::ZapretManual),
                _ => return self.finish(),
            },
            OnboardingStep::ZapretPath => {
                let path = self.zapret_path.trim();
                if path.is_empty() {
                    self.zapret_error = Some("Укажи путь к установленному Zapret".to_string());
                    return OnboardingCommand::None;
                }
                return OnboardingCommand::PlanZapret(PathBuf::from(path));
            }
            OnboardingStep::ZapretReview => {
                if let Some(plan) = self.zapret_plan.clone() {
                    return OnboardingCommand::ApplyZapret(Box::new(plan));
                }
                self.zapret_error =
                    Some("План изменений потерялся, проверь путь снова".to_string());
                self.go(OnboardingStep::ZapretPath);
            }
            OnboardingStep::ZapretManual => return self.finish(),
            OnboardingStep::Complete => return OnboardingCommand::Finish,
        }
        OnboardingCommand::None
    }

    pub fn soundcloud_checked(&mut self, access: SoundCloudAccess) -> OnboardingCommand {
        if access.is_reachable() {
            return self.finish();
        }
        self.soundcloud_problem = match access {
            SoundCloudAccess::Unreachable { reason } => Some(reason),
            SoundCloudAccess::Reachable { .. } => None,
        };
        self.go(OnboardingStep::ZapretChoice);
        OnboardingCommand::None
    }

    pub fn zapret_planned(&mut self, result: Result<ZapretPlan, String>) {
        match result {
            Ok(plan) => {
                self.zapret_plan = Some(plan);
                self.go(OnboardingStep::ZapretReview);
            }
            Err(error) => {
                self.zapret_error = Some(error);
                self.step = OnboardingStep::ZapretPath;
            }
        }
    }

    pub fn zapret_applied(&mut self, result: Result<(), String>) -> OnboardingCommand {
        match result {
            Ok(()) => self.finish(),
            Err(error) => {
                self.zapret_error = Some(error);
                OnboardingCommand::None
            }
        }
    }

    pub fn result(&self) -> OnboardingResult {
        OnboardingResult {
            account_mode: self.account_mode,
            soundcloud_enabled: self.soundcloud_enabled,
            yandex_enabled: self.yandex_enabled,
            audio_output: self.audio_output.clone(),
        }
    }

    fn go(&mut self, step: OnboardingStep) {
        self.step = step;
        self.selected = 0;
    }

    fn finish(&mut self) -> OnboardingCommand {
        self.go(OnboardingStep::Complete);
        OnboardingCommand::Finish
    }

    fn option_count(&self) -> usize {
        match self.step {
            OnboardingStep::Account => 2,
            OnboardingStep::Providers | OnboardingStep::ZapretChoice => 3,
            OnboardingStep::Audio => self.audio_outputs.len(),
            _ => 1,
        }
    }
}

fn default_zapret_path() -> String {
    if cfg!(target_os = "linux") {
        "/opt/zapret".to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reachable_soundcloud_finishes_without_a_zapret_sales_pitch() {
        let mut state = reach_probe();

        let command = state.soundcloud_checked(SoundCloudAccess::Reachable { status: 401 });

        assert_eq!(command, OnboardingCommand::Finish);
        assert_eq!(state.step, OnboardingStep::Complete);
        assert!(state.soundcloud_problem.is_none());
    }

    #[test]
    fn failed_probe_offers_three_honest_choices() {
        let mut state = reach_probe();

        let command = state.soundcloud_checked(SoundCloudAccess::Unreachable {
            reason: "сеть прилегла".to_string(),
        });

        assert_eq!(command, OnboardingCommand::None);
        assert_eq!(state.step, OnboardingStep::ZapretChoice);
        state.select_next();
        state.select_next();
        assert_eq!(state.confirm(), OnboardingCommand::Finish);
    }

    #[test]
    fn provider_switches_are_not_radio_buttons_from_the_nineties() {
        let mut state = OnboardingState::new(None);
        state.confirm();
        state.selected = 1;
        state.confirm();
        assert_eq!(state.step, OnboardingStep::Providers);

        state.toggle();
        assert!(!state.soundcloud_enabled);
        state.select_next();
        state.toggle();
        assert!(!state.yandex_enabled);
    }

    #[test]
    fn zapret_path_accepts_typing_and_refuses_whitespace() {
        let mut state = OnboardingState::new(None);
        state.step = OnboardingStep::ZapretPath;
        state.zapret_path = "   ".to_string();

        assert_eq!(state.confirm(), OnboardingCommand::None);
        assert!(state.zapret_error.is_some());
        state.zapret_path.clear();
        for value in "C:\\zapret".chars() {
            state.input(value);
        }
        assert_eq!(
            state.confirm(),
            OnboardingCommand::PlanZapret(PathBuf::from("C:\\zapret"))
        );
    }

    #[test]
    fn audio_step_keeps_default_and_real_devices() {
        let mut state = OnboardingState::with_audio_outputs(
            None,
            vec!["Колонки".to_string(), "Наушники".to_string()],
        );
        state.step = OnboardingStep::Audio;
        state.select_next();
        state.select_next();

        assert_eq!(state.confirm(), OnboardingCommand::ProbeSoundCloud);
        assert_eq!(state.audio_output.as_deref(), Some("Наушники"));
    }

    fn reach_probe() -> OnboardingState {
        let mut state = OnboardingState::new(None);
        state.confirm();
        state.selected = 1;
        state.confirm();
        state.selected = 2;
        state.confirm();
        assert_eq!(state.confirm(), OnboardingCommand::ProbeSoundCloud);
        state
    }
}
