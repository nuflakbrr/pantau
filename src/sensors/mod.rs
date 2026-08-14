pub mod battery;
pub mod cpu;
pub mod disk;
pub mod display_refresh;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod public_ip;
pub mod smc;
pub mod sys;
pub mod system;
pub mod thermal;
pub mod wifi;

use std::collections::HashMap;

// Processor/Memory are only constructed in this module's own tests today —
// `main.rs` tags every row `SensorGroup::System` regardless of real
// category. Kept (not deleted) since `SensorId` is meant to categorize by
// real group; narrowing the enum would be a design change, not a cleanup.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensorGroup {
    Processor,
    Memory,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SensorId {
    pub group: SensorGroup,
    pub key: &'static str,
}

impl SensorId {
    pub const fn new(group: SensorGroup, key: &'static str) -> Self {
        Self { group, key }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LastReading {
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct SensorReading {
    pub id: SensorId,
    pub value: f64,
}

/// Dedupe-on-change: a sensor value only triggers a UI rebuild when it
/// differs from the previous tick's reading. Checked once per poll before
/// the menu is rebuilt — skips a full NSMenu rebuild when nothing moved.
#[derive(Debug, Default)]
pub struct SensorHistory {
    last: HashMap<SensorId, LastReading>,
}

impl SensorHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if any reading differs from what was stored last tick,
    /// and updates the stored history to the new readings.
    pub fn update(&mut self, readings: &[SensorReading]) -> bool {
        let mut changed = false;
        for reading in readings {
            let entry = self.last.get(&reading.id);
            if entry.map(|r| r.value) != Some(reading.value) {
                changed = true;
            }
            self.last.insert(reading.id.clone(), LastReading { value: reading.value });
        }
        changed
    }
}

/// Average/min/max across all readings found in one sensor category —
/// used for groups with multiple instances (per-core CPU, multi-interface
/// network, multi-volume storage).
#[derive(Debug, Clone, Copy)]
pub struct GroupStats {
    pub avg: f64,
    pub min: f64,
    pub max: f64,
}

pub fn aggregate_group(values: &[f64]) -> Option<GroupStats> {
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().sum();
    let avg = sum / values.len() as f64;
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Some(GroupStats { avg, min, max })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_flags_change_on_first_insert() {
        let mut history = SensorHistory::new();
        let id = SensorId::new(SensorGroup::Processor, "cpu_total");
        let readings = vec![SensorReading {
            id: id.clone(),
            value: 12.5,
        }];
        assert!(history.update(&readings));
    }

    #[test]
    fn history_skips_rebuild_when_unchanged() {
        let mut history = SensorHistory::new();
        let id = SensorId::new(SensorGroup::Processor, "cpu_total");
        let readings = vec![SensorReading {
            id: id.clone(),
            value: 12.5,
        }];
        history.update(&readings);
        assert!(!history.update(&readings));
    }

    #[test]
    fn history_flags_change_when_value_moves() {
        let mut history = SensorHistory::new();
        let id = SensorId::new(SensorGroup::Processor, "cpu_total");
        history.update(&[SensorReading {
            id: id.clone(),
            value: 12.5,
        }]);
        assert!(history.update(&[SensorReading {
            id: id.clone(),
            value: 13.0,
        }]));
    }

    #[test]
    fn aggregate_group_computes_avg_min_max() {
        let stats = aggregate_group(&[10.0, 20.0, 30.0]).unwrap();
        assert_eq!(stats.avg, 20.0);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 30.0);
    }
}
