//! Readers for Phantom Dust (2017 PC/UWP) data files.
//!
//! Layouts are documented by the community (Phantom Dust modding wiki and the
//! Esper-RE research project). This crate is an independent Rust
//! implementation; it ships no game data.

pub mod alr;
pub mod anim;
pub mod bytes;
pub mod capsprites;
pub mod charaset;
pub mod character;
pub mod collision;
pub mod effect;
pub mod effect_archive;
pub mod epa;
pub mod gltf;
pub mod pose;
pub mod skills;
pub mod ssb;
pub mod ssb_vm;
pub mod skirt;
pub mod sound;
pub mod stage_effects;
pub mod stagedat;
pub mod stageset;
pub mod stx;
pub mod surface;
pub mod texture;
