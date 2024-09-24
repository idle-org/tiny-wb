//! The "lib" used for all scan related functions

/// The default start value for a scan
static START: usize = 240_000_000;
/// The default stop value for a scan
static STOP: usize = 959_000_000;
/// The offset used for sending data
static OFFSET: usize = 128;
/// The base delay used inbetween commands
static SHORT_WAIT: u64 = 20;
/// A long delay used to wait for alignment
static LONG_WAIT: u64 = 200;
/// The standard timeout time
static TIMEOUT: u64 = 2000;

use crate::structs::TinyError;
use memchr::memmem;
use packed_struct::prelude::*;
use serialport::SerialPort;
use std::{
    fs::File,
    io::{BufWriter, Read, Write},
    sync::{Arc, Mutex, OnceLock},
    thread::{self},
    time::Duration,
};

use chrono::Local;

/// A single datapoint, using a struct might be overkill but msb/lsb could become a problem in the future
#[derive(PackedStruct, Debug)]
#[packed_struct(endian = "lsb")]
pub struct DataPoint {
    /// The alignement character 'x'
    c: u8,
    /// The value sent with as a u16
    value: u16,
}

/// A static memory lookup function to find the eof string with speed
/// This will construct a lookup fonction for "ch>"
static EOF: OnceLock<memmem::Finder<'_>> = OnceLock::new();

/// Scan a specific range and returns the scan result
#[allow(clippy::too_many_arguments)]
pub(crate) fn scan_with_progress(
    progress: Arc<Mutex<usize>>,
    scan_info: Arc<Mutex<String>>,
    scan_message: Arc<Mutex<String>>,
    return_data: Arc<Mutex<Vec<(f64, f64)>>>,
    port: String,
    start: usize,
    stop: usize,
    points: usize,
) -> Result<(), TinyError> {
    // TODO: Pass the connection status mutex to advance it
    *scan_message.lock().unwrap() = "Oppening connection".to_string();
    let port = serialport::new(port, 115_200)
        .timeout(Duration::from_millis(3000))
        .open(); // .expect("Failed to open port");
    if let Err(e) = port {
        // *scan_message.lock().unwrap() = "Noo".to_string();
        return Err(TinyError::PortConnection(e));
    }
    let mut port = port.unwrap();
    *scan_message.lock().unwrap() = "Initialisation...".to_string();
    *progress.lock().unwrap() = 5;
    initialize(&mut port);

    *progress.lock().unwrap() = 15;
    *scan_message.lock().unwrap() = "Acquiring version & info".to_string();
    let v = send_cmd(&mut port, "version\r", SHORT_WAIT);
    let i = send_cmd(&mut port, "info\r", SHORT_WAIT);
    // TODO: have a more thourough version check
    if v.is_empty() || i.is_empty() {
        return Err(TinyError::NoVersionFound);
    }
    *scan_info.lock().unwrap() = format!("{}{}", v, i);

    *progress.lock().unwrap() = 16;
    *scan_message.lock().unwrap() = "Setting zero...".to_string();
    send_cmd(&mut port, &format!("zero {}\r", OFFSET), SHORT_WAIT);

    *progress.lock().unwrap() = 17;
    *scan_message.lock().unwrap() = "Setting up marker".to_string();
    send_cmd(&mut port, "marker\r", SHORT_WAIT);
    send_cmd(&mut port, "pause\r", SHORT_WAIT);

    *progress.lock().unwrap() = 18;
    *scan_message.lock().unwrap() = "Setting up scan & mode".to_string();
    // return Err(TinyError::ConfigNotFound);
    // panic!();
    let mode = mode_from(start, stop)?;
    send_cmd(&mut port, &format!("mode {} input\r", mode), SHORT_WAIT);

    *progress.lock().unwrap() = 19;
    *scan_message.lock().unwrap() = "Setting up sweep".to_string();
    send_cmd(&mut port, &format!("sweep start {}\r", START), SHORT_WAIT);
    send_cmd(&mut port, &format!("sweep stop {}\r", STOP), SHORT_WAIT);

    *progress.lock().unwrap() = 20;
    *scan_message.lock().unwrap() = "Starting scan...".to_string();
    send_cmd(&mut port, "sweep\r", SHORT_WAIT);
    send_cmd(
        &mut port,
        &format!("scan {} {} 290 101\r", start, stop),
        TIMEOUT,
    );

    *progress.lock().unwrap() = 22;
    let name = scanraw(
        &mut port,
        progress.clone(),
        return_data.clone(),
        start,
        stop,
        points,
    )?;

    *scan_message.lock().unwrap() = format!("Scan saved as {}", name);
    send_cmd(&mut port, "resume\r", SHORT_WAIT);

    wait_sync(LONG_WAIT);
    *progress.lock().unwrap() = 100;
    Ok(())
}

/// Get the mode of operation from the start & stop scan values
fn mode_from(start: usize, stop: usize) -> Result<String, TinyError> {
    // TODO: Switch to low if possible with another non overlaping pattern
    if start > stop {
        return Err(TinyError::BadScanFrequencies);
    }
    match (start, stop) {
        (240_000_000..=960_000_000, 240_000_000..=960_000_000) => Ok("high".to_string()),
        (0..240_000_000, 0..240_000_000) => Ok("low".to_string()),
        _ => Err(TinyError::BadScanFrequencies),
    }
}

/// A (lame) function used for initialisation:
/// Wait's for a whole timeout for data stuck in the buffer and dumps it
fn initialize(port: &mut Box<dyn SerialPort>) {
    wait_sync(SHORT_WAIT);
    read_port(port);
    wait_sync(SHORT_WAIT)
}

/// Bad synchronisation method to ensure data is truthfully sent/received
fn wait_sync(duration: u64) {
    thread::sleep(Duration::from_millis(duration));
}

/// Scan raw frequencies
fn scanraw(
    port: &mut Box<dyn SerialPort>,
    progress: Arc<Mutex<usize>>,
    return_data: Arc<Mutex<Vec<(f64, f64)>>>,
    start: usize,
    stop: usize,
    points: usize,
) -> Result<String, TinyError> {
    unsafe { send_raw(port, &format!("scanraw {} {} {}\r", start, stop, points)) }
    wait_sync(SHORT_WAIT);
    let (name, val) = read_scanraw(
        port,
        progress,
        points,
        start,
        (stop - start) as f64 / points as f64,
    )?;
    *return_data.lock().unwrap() = val;
    wait_sync(LONG_WAIT);
    Ok(name)
}

/// Send a command and reads it to a string
fn send_cmd(port: &mut Box<dyn SerialPort>, command: &str, wait: u64) -> String {
    unsafe { send_raw(port, command) }
    wait_sync(wait);
    let r = read_utf8(port);
    wait_sync(LONG_WAIT);
    r
}

/// Send a command without reading the output buffer
/// This command will produce unknown results if the output buffer is never read
unsafe fn send_raw(port: &mut Box<dyn SerialPort>, command: &str) {
    let output = command.as_bytes();
    let _r = port.write(output).expect("Write failed!");
}

/// Reads from port and tries to convert it to string with loss
fn read_utf8(port: &mut Box<dyn SerialPort>) -> String {
    String::from_utf8_lossy(&read_port(port)).to_string()
}

/// Reads the serial port until it reaches EOF
fn read_port_with_progress(
    port: &mut Box<dyn SerialPort>,
    expected: usize,
    progress: Arc<Mutex<usize>>,
) -> Vec<u8> {
    let mut serial_buf: Vec<u8> = vec![0; 64];
    let mut res = Vec::new();
    let finder = EOF.get_or_init(|| memmem::Finder::new("ch>"));
    while let Ok(r) = port.read(serial_buf.as_mut_slice()) {
        *progress.lock().unwrap() = (100usize.saturating_sub(
            (80 * (expected.saturating_sub(res.len())).clamp(0, expected)) / expected,
        ))
        .clamp(20, 99);
        if let Some(pos) = finder.find(&serial_buf) {
            res.extend_from_slice(&serial_buf[..pos]);
            break;
        }
        res.extend_from_slice(&serial_buf[..r]);
    }
    res
}

/// Read port without updating progress
fn read_port(port: &mut Box<dyn SerialPort>) -> Vec<u8> {
    let mut serial_buf: Vec<u8> = vec![0; 256];
    let mut res = Vec::new();
    let finder = EOF.get_or_init(|| memmem::Finder::new("ch>"));
    while let Ok(r) = port.read(serial_buf.as_mut_slice()) {
        if let Some(pos) = finder.find(&serial_buf) {
            res.extend_from_slice(&serial_buf[..pos]);
            break;
        }
        res.extend_from_slice(&serial_buf[..r]);
    }
    res
}

/// Scan for a raw frequencies, assumes everything is setup and the scan can proceed
/// Waits the normal start Character '{' and begins treating data as normal
///
/// Returns a struct summerizing the information processed
fn read_scanraw(
    port: &mut Box<dyn SerialPort>,
    progress: Arc<Mutex<usize>>,
    points: usize,
    start: usize,
    increment: f64,
) -> Result<(String, Vec<(f64, f64)>), TinyError> {
    let v = read_port_with_progress(port, points * 3 + 6, progress);
    // Find the { marking the beggining
    parse_data(v, points, start, increment)
}

/// A utility function used for parsing the raw data of a scan
pub fn parse_data(
    v: Vec<u8>,
    points: usize,
    start: usize,
    increment: f64,
) -> Result<(String, Vec<(f64, f64)>), TinyError> {
    // let mut values: Vec<u16> = Vec::with_capacity(points);
    let name = format!("scan_{}.csv", Local::now().format("%Y%m%d %H_%M_%S"));
    let mut out = BufWriter::new(File::create(&name)?);
    let mut csvdata = Vec::with_capacity(points);
    if let Some(begining) = memchr::memchr(b'{', &v) {
        // Finds the } marking the end
        if let Some(end) = memchr::memchr(b'}', &v) {
            let data = &v[begining + 1..end];
            for (i, slice) in data.chunks_exact(3).enumerate() {
                let s: &[u8; 3] = &[slice[0], slice[1], slice[2]];
                let d = DataPoint::unpack(s)?;
                let (pos, val) = (
                    (i as f64 * increment + start as f64) / 1000000.,
                    u16_to_f64(d.value),
                );

                csvdata.push((pos, val));
                writeln!(out, "{},{}", pos, val)?;
            }
            // TODO: Dont return use mutex
            Ok((name, csvdata))
        } else {
            Err(TinyError::ParseEnd("}".to_owned()))
        }
    } else {
        Err(TinyError::ParseBeggining("{".to_owned()))
    }
}

/// Utility function converting the received data as u16 to float data
fn u16_to_f64(v: u16) -> f64 {
    (v as f64) / 32. - OFFSET as f64
}
