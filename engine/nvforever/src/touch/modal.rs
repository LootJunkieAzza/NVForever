//! Generic overlay capture for dedicated terminal and companion touch screens.
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
}
impl Bridge<'_> {
    pub(super) fn refresh(&mut self) -> bool {
        let terminal = self.terminal.as_deref().zip(self.terminal_settings.as_deref())
            .is_some_and(|(screen, settings)| nvf_play::terminal::touch::captures(settings, screen));
        let wheel = self.wheel.as_deref().zip(self.wheel_settings.as_deref())
            .is_some_and(|(screen, settings)| nvf_play::companion_wheel::touch::captures(settings, screen));
        let active = terminal || wheel;
        // The first frame after closure still owns the old screen's input batch.
        self.state.draining = self.state.active && !active;
        self.state.active = active;
        self.state.blocking()
    }
}
