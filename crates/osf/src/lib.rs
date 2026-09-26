//! Library surface for the `osf` binary: the layered config, the coding-agent
//! hook handlers, every lint, the repository scan, and the verify gate.
//! Split out so an integration test can call the same code the binary
//! runs, instead of shelling out to it.

pub mod agents;
pub mod answer;
pub mod check;
pub mod checkpoint;
pub mod config;
pub mod exclude;
mod git;
pub use git::{scrub_git_env, scrub_git_env_for_dir};
pub mod githooks;
pub mod hook;
pub mod journal;
pub mod lenses;
pub mod lints;
pub mod moon;
pub mod process;
pub mod quotes;
pub mod reducer;
pub mod repository;
pub mod review;
pub mod review_context;
pub mod review_run;
pub mod reviewers;
pub mod risk;
pub mod scan;
pub mod status;
#[cfg(test)]
mod test_support;
pub mod verify;
