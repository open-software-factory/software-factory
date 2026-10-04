//! Library surface for the `osf` binary: the layered config, the coding-agent
//! hook handlers, every lint, the repository scan, and the verify gate.
//! Split out so an integration test can call the same code the binary
//! runs, instead of shelling out to it.

pub mod agents;
pub mod assets;
pub mod changeset_risk;
pub mod changeset_tests;
pub mod check;
pub mod checkpoint;
pub mod config;
pub mod exclude;
pub mod forge;
pub mod git;
pub use git::{scrub_git_env, scrub_git_env_for_dir};
pub mod githooks;
pub mod github;
pub mod hook;
pub mod journal;
pub mod lints;
pub mod marker;
pub mod moon;
pub mod pr_status;
pub mod pr_tree;
pub mod repository;
pub mod review;
pub mod scan;
pub mod section;
#[cfg(test)]
mod test_support;
pub mod verify;
