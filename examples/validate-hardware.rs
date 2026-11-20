//! Guided, conservative hardware acceptance for tinySA Basic and Ultra.

use std::{
    collections::HashSet,
    env,
    error::Error,
    fs,
    io::{self, Write},
    path::PathBuf,
    thread,
    time::Duration,
};
use tiny_wb::{
    acquisition::{acquire, ScanEvent},
    config::{Config, FrequencyAxis, ModelSelection, OutputConfig},
    connection::open_connection,
    export::save_csv,
    protocol::{Sample, ScanRequest, BASIC_RAW_LEVEL_OFFSET_DB, ULTRA_RAW_LEVEL_OFFSET_DB},
    transport::Shell,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Model {
    Basic,
    Ultra,
    Zs406,
}

impl Model {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "basic" => Some(Self::Basic),
            "ultra" => Some(Self::Ultra),
            "zs406" => Some(Self::Zs406),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Ultra => "Ultra",
            Self::Zs406 => "ZS406",
        }
    }

    fn raw_level_offset_db(self) -> u16 {
        match self {
            Self::Basic => BASIC_RAW_LEVEL_OFFSET_DB,
            Self::Ultra | Self::Zs406 => ULTRA_RAW_LEVEL_OFFSET_DB,
        }
    }

    fn scans(self) -> [(&'static str, ScanRequest, bool, bool); 2] {
        let low = ScanRequest {
            start_hz: 240_000_000,
            stop_hz: 300_000_000,
            points: 101,
            repeat: 1,
        };
        let second = match self {
            Self::Basic => ScanRequest {
                start_hz: 350_000_000,
                stop_hz: 400_000_000,
                ..low
            },
            // Generic Ultra validation stays on the shared low-input path.
            Self::Ultra => low,
            Self::Zs406 => ScanRequest {
                start_hz: 900_000_000,
                stop_hz: 5_400_000_000,
                points: 101,
                repeat: 1,
            },
        };
        match self {
            Self::Zs406 => [
                (
                    "normal",
                    ScanRequest {
                        start_hz: 100_000,
                        stop_hz: 900_000_000,
                        points: 101,
                        repeat: 1,
                    },
                    false,
                    false,
                ),
                ("ultra", second, false, true),
            ],
            Self::Basic | Self::Ultra => [
                ("legacy", low, true, false),
                ("direct", second, false, false),
            ],
        }
    }
}

struct Arguments {
    model: Model,
    port: Option<String>,
    output: PathBuf,
}

fn usage() -> &'static str {
    "usage: validate-hardware <basic|ultra|zs406> [--port DEVICE] [--output DIRECTORY]"
}

fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<Arguments, String> {
    let mut arguments = arguments.into_iter();
    let model = arguments
        .next()
        .as_deref()
        .and_then(Model::parse)
        .ok_or_else(|| usage().to_owned())?;
    let mut port = None;
    let mut output = PathBuf::from("hardware-validation");
    while let Some(option) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {option}\n{}", usage()))?;
        match option.as_str() {
            "--port" if port.is_none() && !value.is_empty() => port = Some(value),
            "--output" if !value.is_empty() => output = PathBuf::from(value),
            _ => return Err(format!("invalid option: {option}\n{}", usage())),
        }
    }
    Ok(Arguments {
        model,
        port,
        output,
    })
}

fn prompt(message: &str) -> io::Result<String> {
    print!("{message}");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_owned())
}

fn ports() -> Result<Vec<String>, serialport::Error> {
    Ok(serialport::available_ports()?
        .into_iter()
        .map(|port| port.port_name)
        .collect())
}

fn select_port(explicit: Option<String>) -> Result<String, Box<dyn Error>> {
    if let Some(port) = explicit {
        prompt("Disconnect every RF source, plug in and power on the tinySA, then press Enter: ")?;
        return Ok(port);
    }

    prompt("Disconnect the tinySA USB cable and every RF source, then press Enter: ")?;
    let before: HashSet<_> = ports()?.into_iter().collect();
    prompt("Plug in and power on the tinySA, then press Enter: ")?;
    thread::sleep(Duration::from_millis(500));
    let available = ports()?;
    let added: Vec<_> = available
        .iter()
        .filter(|port| !before.contains(*port))
        .cloned()
        .collect();
    if added.len() == 1 {
        println!("Detected {}", added[0]);
        return Ok(added[0].clone());
    }
    if available.len() == 1 {
        println!("Using the only available serial port: {}", available[0]);
        return Ok(available[0].clone());
    }
    if available.is_empty() {
        return Err("no serial port appeared; check the USB data cable and permissions".into());
    }
    println!("Select the tinySA serial port:");
    for (index, port) in available.iter().enumerate() {
        println!("  {}. {port}", index + 1);
    }
    let selection: usize = prompt("Port number: ")?.parse()?;
    available
        .get(
            selection
                .checked_sub(1)
                .ok_or("port number must be at least 1")?,
        )
        .cloned()
        .ok_or_else(|| "port number is outside the displayed list".into())
}

fn validate_samples(samples: &[Sample], request: &ScanRequest) -> Result<(), Box<dyn Error>> {
    if samples.len() != request.points {
        return Err(format!(
            "expected {} samples, received {}",
            request.points,
            samples.len()
        )
        .into());
    }
    if samples.iter().any(|sample| {
        !sample.frequency_mhz.is_finite()
            || !sample.level_dbm.is_finite()
            || sample.frequency_mhz < request.start_hz as f64 / 1_000_000.0
            || sample.frequency_mhz > request.stop_hz as f64 / 1_000_000.0
    }) {
        return Err("CSV data contains a non-finite or out-of-range value".into());
    }
    Ok(())
}

fn safe_identity(identity: &str) -> String {
    identity
        .chars()
        .filter(|character| *character == '\n' || *character == '\r' || !character.is_control())
        .collect()
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments = parse_arguments(env::args().skip(1)).map_err(io::Error::other)?;
    println!(
        "tinySA {} hardware validation\nNo RF source should be connected during this communications test.",
        arguments.model.name()
    );
    let port = select_port(arguments.port)?;
    let session = arguments.output.join(format!(
        "{}-{}",
        arguments.model.name().to_ascii_lowercase(),
        jiff::ZonedDateTime::now().strftime("%Y%m%d-%H%M%S-%6f")
    ));
    fs::create_dir_all(&session)?;

    let mut report = format!(
        "requested_model={}\nport={}\nbaudrate=115200\n",
        arguments.model.name(),
        port
    );
    let mut identity = String::new();
    for (name, request, legacy_pre_scan, enable_ultra_mode) in arguments.model.scans() {
        println!(
            "Running {name} scan: {}–{} MHz, {} points...",
            request.start_hz / 1_000_000,
            request.stop_hz / 1_000_000,
            request.points
        );
        let mut config = Config::default();
        config.port.clone_from(&port);
        config.start_frequency = (request.start_hz / 1_000_000) as usize;
        config.stop_frequency = (request.stop_hz / 1_000_000) as usize;
        config.points = request.points;
        config.repeat = request.repeat;
        config.scan.device_model = match arguments.model {
            Model::Basic => ModelSelection::Basic,
            Model::Ultra | Model::Zs406 => ModelSelection::Ultra,
        };
        config.scan.enable_ultra_mode = enable_ultra_mode;
        config.scan.auto_zero_offset = true;
        config.scan.zero_offset = arguments.model.raw_level_offset_db();
        config.scan.frequency_axis = FrequencyAxis::Firmware;
        config.scan.legacy_pre_scan = legacy_pre_scan;
        config.validate()?;
        let connection = open_connection(&config)?;
        let mut shell = Shell::new(connection, config.serial.max_response_bytes);
        let samples = acquire(
            &mut shell,
            &request,
            &config.scan,
            &config.serial,
            |event| {
                if let ScanEvent::InstrumentInfo(value) = event {
                    identity = safe_identity(&value);
                }
            },
        )?;
        validate_samples(&samples, &request)?;
        let output = OutputConfig {
            directory: session.clone(),
            filename_prefix: format!("{}-{name}", arguments.model.name().to_ascii_lowercase()),
            include_header: true,
        };
        let path = save_csv(&samples, &output)?;
        println!(
            "PASS: saved {} validated samples to {}",
            samples.len(),
            path.display()
        );
        report.push_str(&format!(
            "scan={name},start_hz={},stop_hz={},points={},raw_level_offset_db={},legacy_pre_scan={},enable_ultra_mode={},csv={}\n",
            request.start_hz,
            request.stop_hz,
            request.points,
            config.scan.zero_offset,
            legacy_pre_scan,
            enable_ultra_mode,
            path.display()
        ));
    }

    println!("\nInstrument identity returned by the device:\n{identity}");
    let confirmed = prompt(&format!(
        "Does this match a tinySA {} and is its live sweep running again? [y/N] ",
        arguments.model.name()
    ))?;
    report.push_str(&format!(
        "identity=\n{identity}\noperator_confirmed={}\n",
        confirmed
    ));
    let report_path = session.join(format!(
        "{}-report.txt",
        arguments.model.name().to_ascii_lowercase()
    ));
    fs::write(&report_path, report)?;
    if !matches!(confirmed.as_str(), "y" | "Y" | "yes" | "YES" | "Yes") {
        return Err(format!(
            "operator confirmation failed; partial report: {}",
            report_path.display()
        )
        .into());
    }
    println!(
        "PASS: hardware validation report saved to {}",
        report_path.display()
    );
    if arguments.model == Model::Ultra {
        println!("Ultra result covers only the shared 240–300 MHz low-input path; extended ranges remain unsupported.");
    } else if arguments.model == Model::Zs406 {
        println!("ZS406 result covers normal reception through 900 MHz and opt-in non-harmonic Ultra reception through 5.4 GHz; it does not establish RF accuracy.");
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_wb::device::DeviceModel;

    #[test]
    fn plans_keep_ultra_on_the_shared_low_input_path() {
        assert_eq!(Model::Basic.raw_level_offset_db(), 128);
        assert_eq!(Model::Ultra.raw_level_offset_db(), 174);
        let basic = Model::Basic.scans();
        assert_eq!(
            DeviceModel::Basic.plan(&basic[0].1).unwrap().input_command,
            "mode low input"
        );
        assert_eq!(
            DeviceModel::Basic.plan(&basic[1].1).unwrap().input_command,
            "mode high input"
        );
        let ultra = Model::Ultra.scans();
        assert!(ultra.iter().all(|(_, request, _, enable_ultra_mode)| {
            request.stop_hz < 350_000_000
                && !enable_ultra_mode
                && DeviceModel::Ultra
                    .plan(request)
                    .is_ok_and(|plan| plan.input_command == "mode low input")
        }));
        let zs406 = Model::Zs406.scans();
        assert_eq!(zs406[0].1.stop_hz, 900_000_000);
        assert!(!zs406[0].3);
        assert_eq!(zs406[1].1.stop_hz, 5_400_000_000);
        assert!(zs406[1].3);
        assert!(ultra[0].2);
        assert!(!ultra[1].2);
    }

    #[test]
    fn sample_validation_rejects_bad_count_and_values() {
        let request = ScanRequest {
            start_hz: 240_000_000,
            stop_hz: 300_000_000,
            points: 2,
            repeat: 1,
        };
        let valid = [
            Sample {
                frequency_mhz: 240.0,
                level_dbm: -90.0,
            },
            Sample {
                frequency_mhz: 270.0,
                level_dbm: -80.0,
            },
        ];
        assert!(validate_samples(&valid, &request).is_ok());
        assert!(validate_samples(&valid[..1], &request).is_err());
        let invalid = [
            valid[0],
            Sample {
                frequency_mhz: 270.0,
                level_dbm: f64::NAN,
            },
        ];
        assert!(validate_samples(&invalid, &request).is_err());
    }

    #[test]
    fn arguments_require_a_known_model_and_option_values() {
        assert_eq!(
            parse_arguments(["ultra".into(), "--port".into(), "COM7".into()])
                .unwrap()
                .port
                .as_deref(),
            Some("COM7")
        );
        assert!(parse_arguments(["unknown".into()]).is_err());
        assert!(parse_arguments(["basic".into(), "--port".into()]).is_err());
        assert!(parse_arguments(["basic".into(), "--bad".into(), "x".into()]).is_err());
    }
}
