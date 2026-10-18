//! Local secret references, provider resolution, and environment/terminal
//! adapters.
//! Manual configs, exports, environments and clipboard contents remain
//! plaintext.

/// Provider reference browsing without rendering retrieved field values.
pub mod browser;
/// Pass-through-safe wrapper option parsing.
pub mod cli;
/// Ordered configuration discovery, validation and atomic persistence.
pub mod config;
/// Terminal-aware CLI diagnostics with consistent colors and opt-outs.
pub mod console;
/// Internal managed-file preflight, private staging, and conflict snapshots.
pub mod materialization;
/// Literal clipboard transfer and dotenv export.
pub mod operations;
/// Bounded subprocess capture and cancellation cleanup.
pub mod process;
/// Provider registry, parallel resolution and compatible OTP generation.
pub mod providers;
/// Memory-only, consent-gated managed-file push.
pub mod push;
/// Interactive secret manager with explicit, expiring previews.
pub mod ui;
