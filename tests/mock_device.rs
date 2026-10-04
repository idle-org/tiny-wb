//! Production acquisition exercised against stateful, synthetic instruments.
use std::{
    io::{self, Read, Write},
    net::TcpListener,
    time::Duration,
};
use tiny_wb::{
    acquisition::acquire,
    config::{Backend, Config, FrequencyAxis},
    connection::open_connection,
    mock::{serve_connection, MockConfig, MockFault, MockModel, MockStream},
    protocol::ScanRequest,
    transport::Shell,
};

fn settings() -> Config {
    let mut config = Config::default();
    config.serial.command_delay_ms = 0;
    config.serial.settle_delay_ms = 0;
    config.serial.scan_delay_ms = 0;
    config.scan.frequency_axis = FrequencyAxis::Firmware;
    config
}
fn request() -> ScanRequest {
    ScanRequest {
        start_hz: 240_000_000,
        stop_hz: 360_000_000,
        points: 4,
        repeat: 2,
    }
}
fn drain(device: &mut MockStream) -> Vec<u8> {
    let mut response = Vec::new();
    let mut bytes = [0; 4096];
    while device.pending_bytes() > 0 {
        let count = device.read(&mut bytes).unwrap();
        response.extend_from_slice(&bytes[..count]);
    }
    response
}
fn command(device: &mut MockStream, command: &str) -> String {
    device
        .write_all(format!("{command}\r\n").as_bytes())
        .unwrap();
    String::from_utf8_lossy(&drain(device)).into_owned()
}

#[test]
fn acquisition_survives_bytewise_io_and_restores_sweeping() {
    for echo in [false, true] {
        for legacy in [false, true] {
            let mut device = MockStream::new(MockConfig {
                echo,
                read_chunk_bytes: 1,
                write_chunk_bytes: 1,
                ..MockConfig::default()
            })
            .unwrap();
            let mut config = settings();
            config.scan.legacy_pre_scan = legacy;
            let samples = acquire(
                &mut Shell::new(&mut device, 65536),
                &request(),
                &config.scan,
                &config.serial,
                |_| {},
            )
            .unwrap();
            assert_eq!(samples.len(), 4);
            assert_eq!(samples[2].level_dbm, -32.0);
            assert_eq!(samples[0].level_dbm, -96.0);
            assert!(!device.state().paused);
            assert_eq!(device.state().repeat, 2);
            assert_eq!(device.state().commands.back().unwrap(), "resume");
        }
    }
}

#[test]
fn faults_fail_acquisition_and_attempt_recovery() {
    for fault in [
        MockFault::Reject,
        MockFault::Timeout,
        MockFault::Disconnect,
        MockFault::MissingPrompt,
        MockFault::TruncatedFrame,
        MockFault::BadAlignment,
    ] {
        let mut device = MockStream::new(MockConfig {
            fault,
            ..MockConfig::default()
        })
        .unwrap();
        let config = settings();
        let result = acquire(
            &mut Shell::new(&mut device, 65536),
            &request(),
            &config.scan,
            &config.serial,
            |_| {},
        );
        assert!(result.is_err(), "{fault:?}");
        if fault != MockFault::Disconnect {
            assert!(!device.state().paused, "{fault:?}");
            let result = acquire(
                &mut Shell::new(&mut device, 65536),
                &request(),
                &config.scan,
                &config.serial,
                |_| {},
            );
            assert!(result.is_ok(), "one-shot {fault:?}: {result:?}");
        } else {
            assert!(!device.is_connected());
            assert_eq!(device.read(&mut [0]).unwrap(), 0);
            assert_eq!(
                device.write(b"resume").unwrap_err().kind(),
                io::ErrorKind::BrokenPipe
            );
        }
    }
}

#[test]
fn rejected_pause_and_ultra_high_mode_are_not_silent_successes() {
    for mock in [
        MockConfig {
            fault: MockFault::Reject,
            fault_command: "pause".into(),
            ..MockConfig::default()
        },
        MockConfig {
            model: MockModel::Ultra,
            ..MockConfig::default()
        },
    ] {
        let mut device = MockStream::new(mock).unwrap();
        let config = settings();
        let error = acquire(
            &mut Shell::new(&mut device, 65536),
            &request(),
            &config.scan,
            &config.serial,
            |_| {},
        )
        .unwrap_err();
        assert!(error.to_string().contains("reject"), "{error}");
        assert!(!device.state().paused);
    }
}

#[test]
fn ultra_profile_and_explicit_command_subset() {
    let mut device = MockStream::new(MockConfig {
        model: MockModel::Ultra,
        echo: false,
        ..MockConfig::default()
    })
    .unwrap();
    drain(&mut device);
    assert_eq!(device.state().zero_offset, 174);
    assert!(command(&mut device, "version").contains("Ultra"));
    assert!(command(&mut device, "help").contains("lna ultra"));
    for cmd in [
        "mode input",
        "mode low input",
        "lna on",
        "ultra on",
        "sweep start 1000000000",
        "sweep stop 2000000000",
    ] {
        assert_eq!(command(&mut device, cmd), "ch>");
    }
    assert!(device.state().lna && device.state().ultra);
    device
        .write_all(b"scanraw 1000000000 2000000000 2\r")
        .unwrap();
    let frame = drain(&mut device);
    assert_eq!(frame[0], b'{');
    assert_eq!(frame[1], b'x');
    assert_eq!(frame.len(), 13);
    for cmd in ["lna off", "ultra off"] {
        command(&mut device, cmd);
    }
    assert!(!device.state().lna && !device.state().ultra);
    for cmd in [
        "mode high input",
        "lna auto",
        "reset",
        "scanraw 0 6000000001 2",
    ] {
        assert!(command(&mut device, cmd).contains("error:"));
    }
}

#[test]
fn invalid_commands_do_not_mutate_measurement_state() {
    let mut device = MockStream::new(MockConfig::default()).unwrap();
    drain(&mut device);
    for cmd in [
        "repeat 0",
        "repeat xyz",
        "zero 2048",
        "zero xyz",
        "sweep start -1",
        "sweep stop 960000001",
        "scanraw 10 1 2",
        "scanraw 0 350000000 2",
        "scanraw 0 10 1",
        "scanraw x 10 2",
        "scanraw 0 10 10002",
        "lna on",
    ] {
        assert!(command(&mut device, cmd).contains("error:"), "{cmd}");
    }
    assert_eq!(device.state().repeat, 1);
    assert_eq!(device.state().zero_offset, 128);
    assert_eq!(device.state().start_hz, 240_000_000);
    assert_eq!(device.state().stop_hz, 960_000_000);
    command(&mut device, "mode high input");
    assert!(command(&mut device, "scanraw 0 300000000 2").contains("error:"));
}

#[test]
fn framing_limits_and_history_are_bounded() {
    let mut device = MockStream::new(MockConfig {
        command_limit_bytes: 16,
        echo: false,
        ..MockConfig::default()
    })
    .unwrap();
    assert_eq!(device.read(&mut []).unwrap(), 0);
    assert_eq!(device.write(&[]).unwrap(), 0);
    drain(&mut device);
    assert_eq!(
        device.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(command(&mut device, &"x".repeat(17)).contains("too long"));
    device.write_all(b"\xff\r").unwrap();
    assert!(String::from_utf8(drain(&mut device))
        .unwrap()
        .contains("ASCII"));
    for _ in 0..1030 {
        command(&mut device, "pause");
    }
    assert_eq!(device.state().commands.len(), 1024);
    assert_eq!(command(&mut device, "resume"), "ch>");
    let error = loop {
        if let Err(error) = device.write_all(b"help\r") {
            break error;
        }
    };
    assert_eq!(error.kind(), io::ErrorKind::OutOfMemory);
}

#[test]
fn mock_templates_and_invalid_configuration() {
    Config::from_toml(include_str!("../config/mock-client.toml")).unwrap();
    for source in [
        include_str!("../config/mock-basic.toml"),
        include_str!("../config/mock-ultra.toml"),
    ] {
        toml::from_str::<MockConfig>(source)
            .unwrap()
            .validate()
            .unwrap();
    }
    for source in [
        "read_chunk_bytes=0",
        "write_chunk_bytes=0",
        "command_limit_bytes=1",
        "max_points=1",
        "ultra_max_hz=1",
        "floor_dbm=nan",
        "peak_dbm=-100.0",
        "peak_width_hz=0",
        "fault_command='BAD'",
        "fault='bad_alignment'\nfault_command='pause'",
    ] {
        assert!(
            MockStream::new(toml::from_str(source).unwrap()).is_err(),
            "{source}"
        );
    }
    assert!(toml::from_str::<MockConfig>("unknown=1").is_err());
}

#[test]
fn tcp_backend_runs_real_acquisition_and_closes_cleanly() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = settings();
    config.connection.backend = Backend::Tcp;
    config.connection.tcp_address = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        serve_connection(socket, MockConfig::default()).unwrap()
    });
    {
        let mut shell = Shell::new(open_connection(&config).unwrap(), 65536);
        let samples =
            acquire(&mut shell, &request(), &config.scan, &config.serial, |_| {}).unwrap();
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[2].level_dbm, -32.0);
    }
    assert!(!server.join().unwrap().paused);
}

#[test]
fn stream_server_handles_interrupts_disconnects_and_io_errors() {
    struct Duplex {
        input: std::io::Cursor<Vec<u8>>,
        output: Vec<u8>,
        interrupt: bool,
        fail_write: bool,
        fail_read: bool,
    }
    impl Read for Duplex {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if std::mem::take(&mut self.interrupt) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.fail_read {
                return Err(io::ErrorKind::ConnectionReset.into());
            }
            self.input.read(bytes)
        }
    }
    impl Write for Duplex {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            self.output.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let new_stream = || Duplex {
        input: std::io::Cursor::new(b"pause\rresume\r".to_vec()),
        output: Vec::new(),
        interrupt: true,
        fail_write: false,
        fail_read: false,
    };
    let mut stream = new_stream();
    let state = serve_connection(&mut stream, MockConfig::default()).unwrap();
    assert!(!state.paused);
    assert!(stream.output.starts_with(b"ch>"));
    let state = serve_connection(
        new_stream(),
        MockConfig {
            fault: MockFault::Disconnect,
            fault_command: "pause".into(),
            ..MockConfig::default()
        },
    )
    .unwrap();
    assert_eq!(state.commands.len(), 1);
    let mut stream = new_stream();
    stream.fail_write = true;
    assert_eq!(
        serve_connection(stream, MockConfig::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    let mut stream = new_stream();
    stream.fail_read = true;
    assert_eq!(
        serve_connection(stream, MockConfig::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::ConnectionReset
    );
}
