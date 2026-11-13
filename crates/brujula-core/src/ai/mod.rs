//! The optional AI assistant, without a UI: which provider the user chose, how a request to it is
//! written and its answer read, what the assistant may do with the tabs and when it has to ask.
//!
//! Nothing here talks to the network or to a keyring: the app does that. Everything the assistant
//! can do goes through `policy::decide`, so the rules that keep it safe are in one place and are
//! tested without a window.
//!
//! The assistant is off until the user turns it on, and nothing leaves the computer before that.

pub mod markup;
pub mod page;
pub mod policy;
pub mod prompt;
pub mod settings;
pub mod wire;

pub use settings::{AiSettings, Autonomy, Provider};
