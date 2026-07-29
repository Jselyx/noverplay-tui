pub mod action;
pub mod app;
pub mod config;
pub mod event;
pub mod model;
pub mod secrets;
pub mod storage;
pub mod terminal;
pub mod ui;

pub const APP_NAME: &str = "Noverplay";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
