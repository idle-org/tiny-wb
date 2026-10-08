//! Bounded host-side processing for repeated live scan snapshots.
//!
//! This module owns no device, terminal or file. It retains at most sixteen
//! complete sweeps and one value per point for each hold/decay trace.

use crate::protocol::Sample;
use std::{collections::VecDeque, fmt};

/// Host-side trace calculation shown and exported by live mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceMode {
    /// Rolling arithmetic mean of up to four completed sweeps.
    Average4,
    /// Rolling arithmetic mean of up to sixteen completed sweeps.
    Average16,
    /// Highest value observed at every frequency since capture began.
    MaxHold,
    /// Maximum trace that falls by 1 dB per completed sweep.
    MaxDecay,
    /// Lowest value observed at every frequency since capture began.
    MinHold,
    /// Minimum trace that rises by 1 dB per completed sweep.
    MinDecay,
}

impl TraceMode {
    /// Resolve the live-mode number-key binding.
    pub fn from_key(key: char) -> Option<Self> {
        match key {
            '1' => Some(Self::Average4),
            '2' => Some(Self::Average16),
            '3' => Some(Self::MaxHold),
            '4' => Some(Self::MaxDecay),
            '5' => Some(Self::MinHold),
            '6' => Some(Self::MinDecay),
            _ => None,
        }
    }
}

impl fmt::Display for TraceMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Average4 => "average 4",
            Self::Average16 => "average 16",
            Self::MaxHold => "max hold",
            Self::MaxDecay => "max decay",
            Self::MinHold => "min hold",
            Self::MinDecay => "min decay",
        })
    }
}

/// A malformed or incompatible sweep supplied to [`TraceBuffer`].
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LiveTraceError {
    /// Live traces require at least one point.
    #[error("a live sweep cannot be empty")]
    Empty,
    /// Every sweep must retain the configured point count.
    #[error("live sweep has {actual} points; expected {expected}")]
    PointCount {
        /// Point count established by the first sweep.
        expected: usize,
        /// Point count in the rejected sweep.
        actual: usize,
    },
    /// A point's frequency changed between repeated snapshots.
    #[error(
        "live sweep frequency changed at point {index}: expected {expected} MHz, got {actual} MHz"
    )]
    Frequency {
        /// Zero-based point index.
        index: usize,
        /// Frequency established by the first sweep, in MHz.
        expected: f64,
        /// Frequency in the rejected sweep, in MHz.
        actual: f64,
    },
    /// Input values must remain finite before processing.
    #[error("live sweep contains a non-finite value at point {index}")]
    NonFinite {
        /// Zero-based point index.
        index: usize,
    },
}

/// Bounded rolling history and derived hold/decay traces.
#[derive(Debug, Default)]
pub struct TraceBuffer {
    history: VecDeque<Vec<Sample>>,
    max_hold: Vec<f64>,
    max_decay: Vec<f64>,
    min_hold: Vec<f64>,
    min_decay: Vec<f64>,
    sweeps: u64,
}

impl TraceBuffer {
    /// Add one complete sweep, rejecting axis or point-count changes.
    ///
    /// # Errors
    ///
    /// Returns [`LiveTraceError`] for an empty sweep, non-finite values, or a
    /// point count or frequency axis that differs from the retained history.
    /// Rejected input does not change the buffer.
    pub fn push(&mut self, samples: Vec<Sample>) -> Result<(), LiveTraceError> {
        if samples.is_empty() {
            return Err(LiveTraceError::Empty);
        }
        for (index, sample) in samples.iter().enumerate() {
            if !sample.frequency_mhz.is_finite() || !sample.level_dbm.is_finite() {
                return Err(LiveTraceError::NonFinite { index });
            }
        }
        if let Some(previous) = self.history.back() {
            if previous.len() != samples.len() {
                return Err(LiveTraceError::PointCount {
                    expected: previous.len(),
                    actual: samples.len(),
                });
            }
            for (index, (expected, actual)) in previous.iter().zip(&samples).enumerate() {
                if expected.frequency_mhz != actual.frequency_mhz {
                    return Err(LiveTraceError::Frequency {
                        index,
                        expected: expected.frequency_mhz,
                        actual: actual.frequency_mhz,
                    });
                }
            }
        }
        if self.history.is_empty() {
            let levels: Vec<_> = samples.iter().map(|sample| sample.level_dbm).collect();
            self.max_hold = levels.clone();
            self.max_decay = levels.clone();
            self.min_hold = levels.clone();
            self.min_decay = levels;
        } else {
            for (index, sample) in samples.iter().enumerate() {
                self.max_hold[index] = self.max_hold[index].max(sample.level_dbm);
                self.min_hold[index] = self.min_hold[index].min(sample.level_dbm);
                self.max_decay[index] = (self.max_decay[index] - 1.0).max(sample.level_dbm);
                self.min_decay[index] = (self.min_decay[index] + 1.0).min(sample.level_dbm);
            }
        }
        self.history.push_back(samples);
        if self.history.len() > 16 {
            self.history.pop_front();
        }
        self.sweeps = self.sweeps.saturating_add(1);
        Ok(())
    }

    /// Number of complete sweeps incorporated since the last reset.
    pub fn sweeps(&self) -> u64 {
        self.sweeps
    }

    /// Produce the selected trace with the original frequency axis.
    pub fn trace(&self, mode: TraceMode) -> Option<Vec<Sample>> {
        let latest = self.history.back()?;
        let levels = match mode {
            TraceMode::Average4 => return Some(self.average(4)),
            TraceMode::Average16 => return Some(self.average(16)),
            TraceMode::MaxHold => &self.max_hold,
            TraceMode::MaxDecay => &self.max_decay,
            TraceMode::MinHold => &self.min_hold,
            TraceMode::MinDecay => &self.min_decay,
        };
        Some(
            latest
                .iter()
                .zip(levels)
                .map(|(sample, level)| Sample {
                    frequency_mhz: sample.frequency_mhz,
                    level_dbm: *level,
                })
                .collect(),
        )
    }

    fn average(&self, count: usize) -> Vec<Sample> {
        let used = self.history.len().min(count);
        let start = self.history.len() - used;
        let latest = self.history.back().expect("average requires one sweep");
        latest
            .iter()
            .enumerate()
            .map(|(index, sample)| Sample {
                frequency_mhz: sample.frequency_mhz,
                level_dbm: self
                    .history
                    .iter()
                    .skip(start)
                    .map(|sweep| sweep[index].level_dbm)
                    .sum::<f64>()
                    / used as f64,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sweep(levels: &[f64]) -> Vec<Sample> {
        levels
            .iter()
            .enumerate()
            .map(|(index, level)| Sample {
                frequency_mhz: 100.0 + index as f64,
                level_dbm: *level,
            })
            .collect()
    }

    fn levels(buffer: &TraceBuffer, mode: TraceMode) -> Vec<f64> {
        buffer
            .trace(mode)
            .unwrap()
            .into_iter()
            .map(|sample| sample.level_dbm)
            .collect()
    }

    #[test]
    fn rolling_averages_are_bounded_to_four_and_sixteen_sweeps() {
        let mut buffer = TraceBuffer::default();
        for level in 1..=20 {
            buffer.push(sweep(&[f64::from(level)])).unwrap();
        }
        assert_eq!(buffer.history.len(), 16);
        assert_eq!(levels(&buffer, TraceMode::Average4), vec![18.5]);
        assert_eq!(levels(&buffer, TraceMode::Average16), vec![12.5]);
        assert_eq!(buffer.sweeps(), 20);
    }

    #[test]
    fn holds_and_one_db_decay_follow_deterministic_sequences() {
        let mut buffer = TraceBuffer::default();
        buffer.push(sweep(&[-10.0, -30.0])).unwrap();
        buffer.push(sweep(&[-20.0, -20.0])).unwrap();
        assert_eq!(levels(&buffer, TraceMode::MaxHold), vec![-10.0, -20.0]);
        assert_eq!(levels(&buffer, TraceMode::MinHold), vec![-20.0, -30.0]);
        assert_eq!(levels(&buffer, TraceMode::MaxDecay), vec![-11.0, -20.0]);
        assert_eq!(levels(&buffer, TraceMode::MinDecay), vec![-20.0, -29.0]);
    }

    #[test]
    fn incompatible_or_non_finite_sweeps_are_rejected_without_mutation() {
        let mut buffer = TraceBuffer::default();
        assert_eq!(buffer.push(Vec::new()), Err(LiveTraceError::Empty));
        buffer.push(sweep(&[-10.0, -20.0])).unwrap();
        assert!(matches!(
            buffer.push(sweep(&[-10.0])),
            Err(LiveTraceError::PointCount { .. })
        ));
        let mut changed = sweep(&[-10.0, -20.0]);
        changed[1].frequency_mhz = 999.0;
        assert!(matches!(
            buffer.push(changed),
            Err(LiveTraceError::Frequency { .. })
        ));
        assert!(matches!(
            buffer.push(sweep(&[f64::NAN, -20.0])),
            Err(LiveTraceError::NonFinite { .. })
        ));
        assert_eq!(buffer.sweeps(), 1);
    }

    #[test]
    fn number_keys_select_every_supported_mode() {
        assert_eq!(TraceMode::from_key('1'), Some(TraceMode::Average4));
        assert_eq!(TraceMode::from_key('2'), Some(TraceMode::Average16));
        assert_eq!(TraceMode::from_key('3'), Some(TraceMode::MaxHold));
        assert_eq!(TraceMode::from_key('4'), Some(TraceMode::MaxDecay));
        assert_eq!(TraceMode::from_key('5'), Some(TraceMode::MinHold));
        assert_eq!(TraceMode::from_key('6'), Some(TraceMode::MinDecay));
        assert_eq!(TraceMode::from_key('7'), None);
    }
}
