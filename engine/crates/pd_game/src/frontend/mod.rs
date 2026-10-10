//! Front end: title screen, versus setup, arsenal (deck) builder, and the
//! result screen. `main.rs` wires these into the app's `AppState` and owns
//! the Bevy glue (spawning/despawning the match's 3D scene); everything
//! here is either pure logic (discovery, arsenals) or a self-contained
//! screen (title, setup, builder, result). See `docs/FRONTEND.md`.

pub mod arsenals;
pub mod builder;
pub mod builder_ui;
pub mod cloud;
pub mod charselect;
pub mod cpuseat;
pub mod discovery;
pub mod input;
pub mod modes;
pub mod online;
pub mod result;
pub mod script;
pub mod player_memory;
pub mod setup;
pub mod sound;
pub mod state;
pub mod title;
pub mod ui;
