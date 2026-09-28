#[cfg(any(target_os = "linux", windows))]
mod app;
#[cfg(any(target_os = "linux", windows))]
mod model;
#[cfg(any(target_os = "linux", windows))]
mod platform;

#[cfg(any(target_os = "linux", windows))]
pub use app::run;
