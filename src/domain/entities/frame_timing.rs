use serde::{Deserialize, Serialize};

/// Timing de un frame individual derivado de eventos crudos del stream `Timeline`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameTiming {
    pub frame_number: u64,
    pub build_duration_us: u64,
    pub raster_duration_us: u64,
    pub is_janky: bool,
}

/// Resumen agregado de rendimiento sobre una ventana de frames
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub frame_count: u32,
    pub janky_count: u32,
    pub avg_build_us: u64,
    pub avg_raster_us: u64,
    pub worst_frame: Option<FrameTiming>,
    pub frames: Vec<FrameTiming>,
}
