//! escribo: annotate screenshots with arrows, boxes, highlights and text.

pub mod app;
pub mod assets;
pub mod doc;
pub mod geom;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod raster;
pub mod redact;
pub mod render;
pub mod settings;
pub mod text;
pub mod theme;
pub mod ui;
