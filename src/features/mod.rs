//! Screens and server functions grouped by feature. Server functions only parse
//! input, resolve the caller and delegate to application use cases.

pub mod account;
pub mod enroll;
pub mod face;
pub mod home;
pub mod login;
pub mod profile;
pub mod reauth;
pub mod register;
pub mod shell;
pub mod ui;
