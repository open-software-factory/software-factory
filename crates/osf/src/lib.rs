//! Library surface for the `osf` binary: the layered config, the coding-agent
//! hook handlers, and every lint. Split out so an integration test can call
//! the same code the binary runs, instead of shelling out to it.

pub mod config;
pub mod hook;
pub mod lint;
