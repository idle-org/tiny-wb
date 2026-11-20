//! CSV serialization and collision-safe file creation, separate from decoding.

use crate::{config::OutputConfig, protocol::Sample};
use jiff::ZonedDateTime;
use std::{
    fs::{self, OpenOptions},
    io::{self, BufRead, BufWriter, Write},
    path::PathBuf,
};

/// Read samples from tiny-wb's two-column CSV representation.
///
/// Both headerless exports and files beginning with
/// `frequency_MHz,level_dBm` are accepted. Blank lines are ignored. Every data
/// row must contain exactly two finite numbers and frequencies must be in
/// nondecreasing order. An empty file is rejected.
///
/// # Errors
/// Returns [`io::ErrorKind::InvalidData`] with a line number for malformed CSV.
pub fn read_csv(reader: impl BufRead) -> io::Result<Vec<Sample>> {
    let mut samples = Vec::new();
    let mut saw_header = false;
    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if samples.is_empty() && !saw_header && line == "frequency_MHz,level_dBm" {
            saw_header = true;
            continue;
        }
        let mut columns = line.split(',');
        let frequency = columns.next();
        let level = columns.next();
        if frequency.is_none() || level.is_none() || columns.next().is_some() {
            return Err(invalid_csv(line_number, "expected exactly two columns"));
        }
        let parse = |value: &str, name: &str| {
            value.trim().parse::<f64>().map_err(|_| {
                invalid_csv(
                    line_number,
                    &format!("invalid {name} number `{}`", value.trim()),
                )
            })
        };
        let frequency_mhz = parse(frequency.unwrap_or_default(), "frequency")?;
        let level_dbm = parse(level.unwrap_or_default(), "level")?;
        if !frequency_mhz.is_finite() || !level_dbm.is_finite() {
            return Err(invalid_csv(line_number, "values must be finite"));
        }
        if samples
            .last()
            .is_some_and(|sample: &Sample| sample.frequency_mhz > frequency_mhz)
        {
            return Err(invalid_csv(
                line_number,
                "frequencies must be in nondecreasing order",
            ));
        }
        samples.push(Sample {
            frequency_mhz,
            level_dbm,
        });
    }
    if samples.is_empty() {
        return Err(invalid_csv(0, "file contains no samples"));
    }
    Ok(samples)
}

/// Build a bounded, contextual malformed-CSV error.
fn invalid_csv(line: usize, message: &str) -> io::Error {
    let location = if line == 0 {
        String::new()
    } else {
        format!(" at line {line}")
    };
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid scan CSV{location}: {message}"),
    )
}

/// Write sample values to any writer, optionally with an explicit units header.
///
/// The caller decides when/how to open and flush a file. Every write error is
/// propagated; decimal formatting uses Rust's locale-independent representation.
pub fn write_csv(
    mut writer: impl Write,
    samples: &[Sample],
    include_header: bool,
) -> io::Result<()> {
    if include_header {
        writeln!(writer, "frequency_MHz,level_dBm")?;
    }
    for sample in samples {
        writeln!(writer, "{},{}", sample.frequency_mhz, sample.level_dbm)?;
    }
    Ok(())
}

/// Save an already validated scan without overwriting any previous acquisition.
///
/// Names use local time plus microseconds and a collision counter. A file is
/// reported only after flushing; failed writes remove the incomplete file.
/// Readers may see the in-progress file, so only the returned path means complete.
pub fn save_csv(samples: &[Sample], settings: &OutputConfig) -> io::Result<PathBuf> {
    fs::create_dir_all(&settings.directory)?;
    let stamp = filename_stamp(&ZonedDateTime::now());
    for suffix in 0..100 {
        let path = settings
            .directory
            .join(format!("{}_{stamp}_{suffix}.csv", settings.filename_prefix));
        let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let mut writer = BufWriter::new(file);
        let result = write_csv(&mut writer, samples, settings.include_header)
            .and_then(|()| writer.flush())
            .and_then(|()| writer.get_ref().sync_all());
        drop(writer);
        if let Err(error) = result {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        return Ok(path);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique scan filename",
    ))
}

/// Preserve the local calendar fields and six fractional digits in export names.
fn filename_stamp(now: &ZonedDateTime) -> String {
    now.strftime("%Y%m%d %H_%M_%S_%6f").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_timestamp_preserves_local_time_and_microseconds() {
        for (input, expected) in [
            (
                "2024-02-29T23:59:58.123456789+05:30[+05:30]",
                "20240229 23_59_58_123456",
            ),
            (
                "2026-01-01T00:00:00.000001-04:00[-04:00]",
                "20260101 00_00_00_000001",
            ),
            ("2026-01-01T00:00:00Z[UTC]", "20260101 00_00_00_000000"),
        ] {
            let now: ZonedDateTime = input.parse().unwrap();
            assert_eq!(filename_stamp(&now), expected);
        }
    }

    #[test]
    fn csv_has_explicit_units_only_when_requested() {
        let samples = [Sample {
            frequency_mhz: 240.5,
            level_dbm: -64.25,
        }];
        let mut bytes = Vec::new();
        write_csv(&mut bytes, &samples, false).unwrap();
        assert_eq!(bytes, b"240.5,-64.25\n");
        bytes.clear();
        write_csv(&mut bytes, &samples, true).unwrap();
        assert_eq!(bytes, b"frequency_MHz,level_dBm\n240.5,-64.25\n");
    }

    #[test]
    fn write_errors_are_propagated() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            write_csv(Broken, &[], true).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn csv_reader_accepts_header_and_headerless_data() {
        let expected = vec![Sample {
            frequency_mhz: 240.5,
            level_dbm: -64.25,
        }];
        assert_eq!(
            read_csv(&b"frequency_MHz,level_dBm\n240.5,-64.25\n"[..]).unwrap(),
            expected
        );
        assert_eq!(read_csv(&b"\n240.5, -64.25\n"[..]).unwrap(), expected);
    }

    #[test]
    fn csv_reader_rejects_empty_malformed_and_unordered_data() {
        for input in [
            "",
            "frequency_MHz,level_dBm\n",
            "240\n",
            "MHz,dBm\n",
            "240,NaN\n",
            "241,-1\n240,-2\n",
        ] {
            let error = read_csv(input.as_bytes()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{input:?}");
        }
    }

    #[test]
    fn scans_do_not_overwrite_each_other() {
        let directory = std::env::temp_dir().join(format!("tiny-wb-export-{}", std::process::id()));
        let settings = OutputConfig {
            directory: directory.clone(),
            filename_prefix: "test".into(),
            include_header: false,
        };
        let samples = [Sample {
            frequency_mhz: 1.0,
            level_dbm: -1.0,
        }];
        let first = save_csv(&samples, &settings).unwrap();
        let second = save_csv(&samples, &settings).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(first).unwrap(), "1,-1\n");
        assert_eq!(fs::read_to_string(second).unwrap(), "1,-1\n");
        fs::remove_dir_all(directory).unwrap();
    }
}
