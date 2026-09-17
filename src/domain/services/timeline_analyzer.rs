use crate::domain::entities::{FrameTiming, PerformanceReport};
use serde_json::Value;

/// Presupuesto de frame por defecto para 60Hz (~16.6ms). Cruzarlo se considera "jank".
pub const DEFAULT_FRAME_BUDGET_US: u64 = 16_600;

/// Deriva timings de frame a partir de eventos crudos del stream `Timeline` del VM Service
/// (formato Chrome Trace Event: `ph` begin/end/complete, `ts`, `name`), en vez de depender de
/// una extension RPC de nombre no verificado (`ext.flutter.timelineEvents`/`getFrameTimings`).
pub struct TimelineAnalyzer;

impl TimelineAnalyzer {
    /// Empareja `Animator::BeginFrame` (proxy del build en el UI thread) y
    /// `Rasterizer::DrawToSurfaces` (proxy del raster en el thread de GPU) para calcular,
    /// frame a frame, cuánto tomó construir y rasterizar. Nombres validados contra una app
    /// Flutter real (Linux desktop, engine actual): el nombre es "DrawToSurfaces" en plural,
    /// no "DrawToSurface" como en documentación/memoria de versiones anteriores del engine.
    pub fn compute_frame_timings(events: &[Value], frame_budget_us: u64) -> Vec<FrameTiming> {
        let build_us = Self::paired_durations(events, "Animator::BeginFrame");
        let raster_us = Self::paired_durations(events, "Rasterizer::DrawToSurfaces");

        let frame_count = build_us.len();
        (0..frame_count)
            .map(|i| {
                let build_duration_us = build_us[i];
                let raster_duration_us = raster_us.get(i).copied().unwrap_or(0);
                FrameTiming {
                    frame_number: i as u64,
                    build_duration_us,
                    raster_duration_us,
                    is_janky: build_duration_us + raster_duration_us > frame_budget_us,
                }
            })
            .collect()
    }

    /// Agrega una lista de `FrameTiming` en un reporte legible por un agente (promedios,
    /// conteo de jank, peor frame) en vez de forzarlo a promediar una lista cruda él mismo.
    pub fn summarize(frames: &[FrameTiming]) -> PerformanceReport {
        let frame_count = frames.len() as u32;
        let janky_count = frames.iter().filter(|f| f.is_janky).count() as u32;
        let avg_build_us = Self::average(frames.iter().map(|f| f.build_duration_us));
        let avg_raster_us = Self::average(frames.iter().map(|f| f.raster_duration_us));
        let worst_frame = frames
            .iter()
            .max_by_key(|f| f.build_duration_us + f.raster_duration_us)
            .cloned();

        PerformanceReport {
            frame_count,
            janky_count,
            avg_build_us,
            avg_raster_us,
            worst_frame,
            frames: frames.to_vec(),
        }
    }

    fn average(values: impl Iterator<Item = u64>) -> u64 {
        let values: Vec<u64> = values.collect();
        if values.is_empty() {
            return 0;
        }
        values.iter().sum::<u64>() / values.len() as u64
    }

    /// Empareja eventos `B`/`E` (o toma `dur` directo de eventos `X` de duración completa)
    /// consecutivos por nombre y devuelve las duraciones resultantes en microsegundos.
    fn paired_durations(events: &[Value], name: &str) -> Vec<u64> {
        let mut durations = Vec::new();
        let mut pending_start: Option<i64> = None;

        for event in events {
            if event.get("name").and_then(|n| n.as_str()) != Some(name) {
                continue;
            }
            let ts = event.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
            match event.get("ph").and_then(|p| p.as_str()) {
                Some("B") => pending_start = Some(ts),
                Some("E") => {
                    if let Some(start) = pending_start.take() {
                        durations.push((ts - start).max(0) as u64);
                    }
                }
                Some("X") => {
                    if let Some(dur) = event.get("dur").and_then(|d| d.as_i64()) {
                        durations.push(dur.max(0) as u64);
                    }
                }
                _ => {}
            }
        }

        durations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn begin_end(name: &str, start: i64, end: i64) -> Vec<Value> {
        vec![
            json!({ "name": name, "ph": "B", "ts": start }),
            json!({ "name": name, "ph": "E", "ts": end }),
        ]
    }

    #[test]
    fn test_computes_frame_timing_from_begin_end_pairs() {
        let mut events = begin_end("Animator::BeginFrame", 1_000, 9_000);
        events.extend(begin_end("Rasterizer::DrawToSurfaces", 9_000, 15_000));

        let frames = TimelineAnalyzer::compute_frame_timings(&events, DEFAULT_FRAME_BUDGET_US);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].build_duration_us, 8_000);
        assert_eq!(frames[0].raster_duration_us, 6_000);
        assert!(!frames[0].is_janky);
    }

    #[test]
    fn test_marks_frame_as_janky_when_over_budget() {
        let mut events = begin_end("Animator::BeginFrame", 0, 12_000);
        events.extend(begin_end("Rasterizer::DrawToSurfaces", 12_000, 20_000));

        let frames = TimelineAnalyzer::compute_frame_timings(&events, DEFAULT_FRAME_BUDGET_US);
        assert!(frames[0].is_janky);
    }

    #[test]
    fn test_summarize_computes_averages_and_worst_frame() {
        let frames = vec![
            FrameTiming {
                frame_number: 0,
                build_duration_us: 4_000,
                raster_duration_us: 4_000,
                is_janky: false,
            },
            FrameTiming {
                frame_number: 1,
                build_duration_us: 20_000,
                raster_duration_us: 10_000,
                is_janky: true,
            },
        ];

        let report = TimelineAnalyzer::summarize(&frames);
        assert_eq!(report.frame_count, 2);
        assert_eq!(report.janky_count, 1);
        assert_eq!(report.avg_build_us, 12_000);
        assert_eq!(report.worst_frame.unwrap().frame_number, 1);
    }
}
