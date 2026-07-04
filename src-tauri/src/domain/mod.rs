//! Domain model: accounts, usage windows, and state transitions.

pub mod account;
pub mod state;
pub mod window;

pub use account::{Account, AccountStatus, Browser, FetchStatus, Plan};
pub use state::{Transition, apply_snapshot, evaluate_status};
pub use window::{Source, UsageSnapshot, Window, WindowKind};
