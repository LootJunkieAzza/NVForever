//! Generic overlay capture for dedicated terminal, companion and dialogue touch screens.
use bevy::{ecs::system::SystemParam, prelude::*};

#[derive(Resource, Default)]
pub(super) struct Capture {
    active: bool,
    draining: bool,
}
impl Capture {
    pub(super) fn blocking(&self) -> bool {
        self.active || self.draining
    }
}

#[derive(SystemParam)]
pub(super) struct Bridge<'w> {
    state: ResMut<'w, Capture>,
    terminal: Option<Res<'w, nvf_play::terminal::TerminalScreen>>,
    terminal_settings: Option<Res<'w, nvf_play::terminal::touch::Settings>>,
    wheel: Option<Res<'w, nvf_play::companion_wheel::WheelScreen>>,
    wheel_settings: Option<Res<'w, nvf_play::companion_wheel::touch::Settings>>,
    lockpick: Option<Res<'w, nvf_play::lockpick::LockpickScreen>>,
    lockpick_options: Option<Res<'w, nvf_play::lockpick::touch::Options>>,
    dialogue: Option<Res<'w, nvf_play::dialogue_touch::TouchDialogue>>,
    play: Option<Res<'w, State<nvf_play::PlayState>>>,
    next: Option<Res<'w, NextState<nvf_play::PlayState>>>,
}
impl Bridge<'_> {
    pub(super) fn refresh(&mut self) -> bool {
        let terminal = self
            .terminal
            .as_deref()
            .zip(self.terminal_settings.as_deref())
            .is_some_and(|(screen, settings)| {
                nvf_play::terminal::touch::captures(settings, screen)
            });
        let wheel = self
            .wheel
            .as_deref()
            .zip(self.wheel_settings.as_deref())
            .is_some_and(|(screen, settings)| {
                nvf_play::companion_wheel::touch::captures(settings, screen)
            });
        let play = match self.next.as_deref() {
            Some(NextState::Pending(next) | NextState::PendingIfDifferent(next)) => Some(*next),
            _ => self.play.as_deref().map(|state| *state.get()),
        };
        let dialogue =
            play.is_some_and(|state| self.dialogue.as_deref().is_some_and(|d| d.captures(state)));
        let lockpick = self.lockpick.as_deref().zip(self.lockpick_options.as_deref())
            .is_some_and(|(screen, options)| nvf_play::lockpick::touch::active(screen, options));
        let active = terminal || wheel || dialogue || lockpick;
        // The first frame after closure still owns the old screen's input batch.
        self.state.draining = self.state.active && !active;
        self.state.active = active;
        self.state.blocking()
    }
}
