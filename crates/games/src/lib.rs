#![deny(missing_docs)]
//! Mini games: team leaderboards for the game rooms people play between tasks.
//!
//! A game room is a native `game` document whose moves and runs live on the
//! sync service. This crate only ranks results, following the hexagonal
//! architecture pattern:
//!
//! - High-score games keep each player's best result; a team leaderboard
//!   ranks the team's current members. Players without a team see their own.
//! - Two-player and party rooms report each finished round once, keyed by
//!   room and round; team leaderboards count outright wins.
//!
//! Results are reported by players' clients, as in any casual web game. The
//! service validates bounds, room edit access, and player membership, but it
//! cannot replay a room's moves.
//!
//! # Architecture
//!
//! - **domain**: models, ports, and the service implementation.
//! - **inbound**: driving adapters (Axum HTTP router).
//! - **outbound**: driven adapters (Postgres repository).

pub mod domain;

#[cfg(feature = "inbound")]
pub mod inbound;

#[cfg(feature = "outbound")]
pub mod outbound;
