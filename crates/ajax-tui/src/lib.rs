#![deny(unsafe_op_in_unsafe_fn)]

mod actions;
mod cockpit_state;
mod facade;
mod input;
mod layout;
mod navigation;
mod palette;
mod rendering;
mod runtime;

#[cfg(test)]
mod architecture;

pub use cockpit_state::{App, CockpitSnapshot};
pub use facade::{render_cockpit, ActionOutcome, CockpitEventHandler, PendingAction};
pub use runtime::{
    run_interactive, run_interactive_with_flash, run_interactive_with_flash_and_refresh,
};

pub(crate) use layout::{feed_screen_rows, feed_top_row};
