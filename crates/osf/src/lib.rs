//! Library surface for the `osf` binary: the layered config, the coding-agent
//! hook handlers, every lint, the repository scan, and the verify gate.
//! Split out so an integration test can call the same code the binary
//! runs, instead of shelling out to it.

pub mod agents;
pub mod assets;
pub mod changeset_risk;
pub mod changeset_tests;
pub mod config;
pub mod exclude;
pub mod git;
pub mod hook;
pub mod lints;
pub mod pr_status;
pub mod repository;
pub mod review;
pub mod scan;
pub mod section;
pub mod verify;
