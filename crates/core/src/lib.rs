//! Shared domain logic for the ReleaseDock.
//!
//! The CLI and desktop app must call this crate instead of duplicating install
//! decisions in two places.

pub mod asset_matcher;
pub mod config;
pub mod install_plan;
pub mod installer;
pub mod integrity;
pub mod manifest;
pub mod release;
pub mod release_policy;
pub mod repo;
pub mod system_proxy;
pub mod windows_install_registry;
