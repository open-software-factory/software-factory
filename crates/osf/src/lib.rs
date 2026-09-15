//! Library surface for the `osf` binary: the layered config, the coding-agent
//! hook handlers, every lint, the repository scan, and the verify gate.
//! Split out so an integration test can call the same code the binary
//! runs, instead of shelling out to it.

pub mod config;
pub mod exclude;
mod git;
pub mod hook;
pub mod lint;
pub mod scan;
pub mod verify;
