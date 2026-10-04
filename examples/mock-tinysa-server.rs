//! Local TCP shell simulator. See docs/mock-device.md for scope and usage.

use std::{
    io::{self, Write},
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    time::Duration,
};
use tiny_wb::mock::{serve_connection, MockConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut address: SocketAddr = "127.0.0.1:7355".parse()?;
    let mut config_path: Option<PathBuf> = None;
    let mut once = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => address = args.next().ok_or("--bind requires IP:port")?.parse()?,
            "--config" => {
                config_path = Some(args.next().ok_or("--config requires a TOML path")?.into())
            }
            "--once" => once = true,
            "--help" => {
                println!("usage: mock-tinysa-server [--bind IP:port] [--config FILE] [--once]");
                return Ok(());
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    let config = match config_path {
        Some(path) => toml::from_str(&std::fs::read_to_string(path)?)?,
        None => MockConfig::default(),
    };
    config.validate()?;
    let listener = TcpListener::bind(address)?;
    println!(
        "Mock tinySA listening on {} ({:?}, synthetic)",
        listener.local_addr()?,
        config.model
    );
    io::stdout().flush()?;
    for connection in listener.incoming() {
        let stream = connection?;
        stream.set_read_timeout(Some(Duration::from_millis(config.session_timeout_ms)))?;
        stream.set_write_timeout(Some(Duration::from_millis(config.session_timeout_ms)))?;
        stream.set_nodelay(true)?;
        if let Err(error) = serve_connection(stream, config.clone()) {
            eprintln!("mock session ended: {error}");
            if once {
                return Err(error.into());
            }
        }
        if once {
            break;
        }
    }
    Ok(())
}
