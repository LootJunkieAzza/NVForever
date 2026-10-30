//! The menus' three-state pointer feedback (none, hovered, pressed) on Bevy 0.20.
//!
//! Bevy 0.20 deprecates `bevy::ui::Interaction` and `bevy::ui::widget::Button` (to be
//! removed in 0.21; its replacements are `picking::hover::Hovered`, `ui::Pressed` and the
//! headless `ui_widgets::Button`). Every menu here reads `Interaction`, and Bevy's
//! `ui_focus_system` still keeps it up to date on any UI node that has one, so the menus
//! keep it for now: this module names the deprecated items once, and the menus import
//! [`Button`] and [`Interaction`] from here instead of the prelude. Moving the menus to
//! `Hovered`/`Pressed` is the work of the Bevy 0.21 upgrade.
#![allow(deprecated)]

use bevy::prelude::{Component, Node};
use bevy::ui::FocusPolicy;

/// Bevy's pointer state of a UI node (none, hovered, pressed).
pub type Interaction = bevy::ui::Interaction;

/// A clickable UI node: blocks the pointer and carries an [`Interaction`], as Bevy's own
/// `Button` did before 0.20.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[require(Node, FocusPolicy::Block, Interaction)]
pub struct Button;
