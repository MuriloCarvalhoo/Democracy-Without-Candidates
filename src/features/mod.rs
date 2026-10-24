//! Screens and server functions grouped by feature. Server functions only parse
//! input, resolve the caller and delegate to application use cases.

pub mod audit;
pub mod home;
pub mod notifications;
pub mod reauth;
pub mod shell;
pub mod ui;
