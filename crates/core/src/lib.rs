//! Shared domain logic for the GitHub Release Manager.
//!
//! The CLI and desktop app must call this crate instead of duplicating install
//! decisions in two places.

pub mod asset_matcher;
pub mod install_plan;
pub mod manifest;
pub mod release;
pub mod repo;
