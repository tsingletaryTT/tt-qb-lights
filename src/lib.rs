// Library interface for tt-qb-lights
// Exposes public API for integration tests and potential library usage

pub mod config;
pub mod monitoring;
pub mod rgb;

// Re-export commonly used types for convenience
pub use config::{Config, ColorThreshold, ColorMappingConfig};
pub use rgb::{RgbColor, color_mapping::ColorMapper};
