//! The "lib" used for all scan related functions

/// The offset used for sending data
static OFFSET: usize = 128;
/// The base delay used inbetween commands
static SHORT_WAIT: u64 = 20;
/// A long delay used to wait for alignment
static LONG_WAIT: u64 = 200;
/// The standard timeout time
static TIMEOUT: u64 = 2000;

use crate::structs::TinyError;
use crate::{info, structs::ConnectionInfo};
use log::{error, trace, warn};
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
    align: u8,
    /// The value sent with as a u16
    value: u16,
}

/// A static memory lookup function to find the eof string with speed
/// This will construct a lookup fonction for "ch>"
static EOF: OnceLock<memmem::Finder<'_>> = OnceLock::new();

/// Scan a specific range and returns the scan result
#[allow(clippy::too_many_arguments)]
pub(crate) fn scan_with_progress(connection_info: ConnectionInfo) -> Result<(), TinyError> {
    info!("Initializying connection: {:#?}", connection_info);
    let mode = mode_from(connection_info.start, connection_info.stop)?;
    connection_info.update_message("Opening connection");
    let port = serialport::new(&connection_info.port, connection_info.baudrate)
        .timeout(Duration::from_millis(3000))
        .open();
    if let Err(e) = port {
        error!("Connection error {}", e);
        connection_info.set_error(Some(TinyError::PortConnection(e.clone()).into()));
        return Err(TinyError::PortConnection(e));
    }
    let mut port = port?;
    connection_info.update_message("Initialisation...");
    connection_info.update_progress(5);
    initialize(&mut port);

    connection_info.update_progress(15);
    connection_info.update_message("Acquiring version & info");
    let v = send_cmd(&mut port, "version\r", SHORT_WAIT)?;
    let i = send_cmd(&mut port, "info\r", SHORT_WAIT)?;
    // let f: String = send_cmd(&mut port, "leveloffset\r", SHORT_WAIT)?;
    // TODO : Also add deviceid, vbat, usart_cfg; threads
    // TODO: Parse this info in a struct and do a version check

    // TODO: have a more thourough version check
    if v.is_empty() || i.is_empty() {
        return Err(TinyError::NoVersionFound);
    }
    connection_info.update_info(format!("{}{}", v, i));

    connection_info.update_message("Setting up repeat");
    send_cmd(
        &mut port,
        &format!("repeat {}\r", connection_info.repeat),
        SHORT_WAIT,
    )?;
    // // TODO: Allow for user configuration
    // send_cmd(&mut port, "calc aver16\r", SHORT_WAIT);

    connection_info.update_progress(16);
    connection_info.update_message("Setting zero...");
    send_cmd(&mut port, &format!("zero {}\r", OFFSET), SHORT_WAIT)?;

    connection_info.update_progress(17);
    connection_info.update_message("Setting up marker");
    send_cmd(&mut port, "marker\r", SHORT_WAIT)?;
    send_cmd(&mut port, "pause\r", SHORT_WAIT)?;

    connection_info.update_progress(18);
    connection_info.update_message("Setting up scan & mode");
    send_cmd(&mut port, &format!("mode {} input\r", mode), SHORT_WAIT)?;

    connection_info.update_progress(19);
    connection_info.update_message("Setting up sweep");
    send_cmd(
        &mut port,
        &format!("sweep start {}\r", connection_info.start),
        SHORT_WAIT,
    )?;
    send_cmd(
        &mut port,
        &format!("sweep stop {}\r", connection_info.stop),
        SHORT_WAIT,
    )?;

    connection_info.update_progress(20);
    connection_info.update_message("Starting scan...");
    send_cmd(&mut port, "sweep\r", SHORT_WAIT)?;
    send_cmd(
        &mut port,
        &format!(
            "scan {} {} 290 101\r",
            connection_info.start, connection_info.stop
        ),
        TIMEOUT,
    )?;

    connection_info.update_progress(22);
    connection_info.update_filename(scanraw(&mut port, connection_info.clone())?);

    connection_info.update_message("Acquiring version & info");
    connection_info.update_message("Scan finished");
    send_cmd(&mut port, "resume\r", SHORT_WAIT)?;

    // wait_sync(LONG_WAIT);
    connection_info.update_progress(100);
    Ok(())
}

/// Get the mode of operation from the start & stop scan values
fn mode_from(start: usize, stop: usize) -> Result<String, TinyError> {
    if start > stop {
        return Err(TinyError::BadScanFrequencies(format!(
            "Start frequency {}Hz is higher than stop frequency {}Hz",
            start, stop
        )));
    }
    match (start, stop) {
        (350_000_000..=960_000_000, 350_000_000..=960_000_000) => Ok("high".to_string()),
        (0..350_000_000, 0..350_000_000) => Ok("low".to_string()),
        (240_000_000..350_000_000, 350_000_000..=960_000_000) => Ok("high".to_string()),
        _ => Err(TinyError::BadScanFrequencies(
            "Scan frequencies must either be in range (0MHz-240MHz) or (240MHz-960MHz)".to_owned(),
        )),
    }
}

/// A (lame) function used for initialisation:
/// Wait's for a whole timeout for data stuck in the buffer and dumps it
fn initialize(port: &mut Box<dyn SerialPort>) {
    wait_sync(SHORT_WAIT);
    let d = read_port(port);
    if !d.is_empty() {
        info!(target: "scan", "Pre-init data found");
        info!(target: "scan", "{:?}", String::from_utf8_lossy(&d).to_string());
    }
    wait_sync(SHORT_WAIT)
}

/// Bad synchronisation method to ensure data is truthfully sent/received
fn wait_sync(duration: u64) {
    thread::sleep(Duration::from_millis(duration));
}

/// Scan raw frequencies
fn scanraw(
    port: &mut Box<dyn SerialPort>,
    connection_info: ConnectionInfo,
) -> Result<String, TinyError> {
    unsafe {
        send_raw(
            port,
            &format!(
                "scanraw {} {} {}\r",
                connection_info.start, connection_info.stop, connection_info.points
            ),
        )?
    }
    wait_sync(SHORT_WAIT);
    let (name, val) = read_scanraw(port, connection_info.clone())?;
    connection_info.update_return_data(val);
    wait_sync(LONG_WAIT);
    Ok(name)
}

/// Send a command and reads it to a string
fn send_cmd(port: &mut Box<dyn SerialPort>, command: &str, wait: u64) -> Result<String, TinyError> {
    unsafe { send_raw(port, command)? }
    wait_sync(wait);
    let r = read_utf8(port);
    wait_sync(LONG_WAIT);
    Ok(r)
}

/// Send a command without reading the output buffer
/// This command will produce unknown results if the output buffer is never read
unsafe fn send_raw(port: &mut Box<dyn SerialPort>, command: &str) -> Result<(), TinyError> {
    let output = command.as_bytes();
    let _r = port.write(output).map_err(TinyError::SendError)?;
    Ok(())
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
        // TODO: Pass the whole object to avoid this unwrap ?
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
// TODO: Find a way to avoid code duplication
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
    connection_info: ConnectionInfo,
) -> Result<(String, Vec<(f64, f64)>), TinyError> {
    let v = read_port_with_progress(
        port,
        connection_info.points * 3 + 6,
        connection_info.mutex_progress(),
    );
    parse_data(
        v,
        connection_info.points,
        connection_info.start,
        (connection_info.stop - connection_info.start) as f64 / (connection_info.points - 1) as f64,
    )
}

/// A utility function used for parsing the raw data of a scan
// TODO: Split save data to another function
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
        let end_tmp = v.len();
        let data = &v[begining + 1..end_tmp]; // -4 since ch>\r will always be ignored
                                              // TODO: Fix weird data
        info!(
            "The begining looks like :{:?}",
            &v[begining - 1..begining + 9]
        );
        info!("The end looks like :{:?}", &v[end_tmp - 10..end_tmp]);

        let mut end = end_tmp;
        let chunks_exact = data.chunks_exact(3);
        // if chunks_exact.remainder().is_empty() {
        //     return Err(TinyError::ParseEnd(
        //         "The data received doesn't have a end char (empty)".to_owned(),
        //     ));
        // } else if chunks_exact.remainder()[0] != b'}' {
        //     error!("Incorrect right end char {:?}", chunks_exact.remainder());
        //     // No end char, bogus
        //     return Err(TinyError::ParseEnd(format!(
        //         "The returned data doesn't have the right end char expected }} found {:?}",
        //         chunks_exact.remainder()
        //     )));
        // }
        for (i, slice) in chunks_exact.enumerate() {
            let s: &[u8; 3] = &[slice[0], slice[1], slice[2]];
            let d = DataPoint::unpack(s)?;
            if d.align == b'}' {
                // Found an end and the rest is garbage
                end = i * 3;
                // TODO: why doesn't it pop ?
                warn!("Found an end at {end} {:?}", &v[end - 4..end]);
                break;
            } else if d.align != b'x' {
                // The alignment char is wrong
                error!("Wrong alignment char expected {} found {}", b'x', d.align);
                return Err(TinyError::ParseEnd("Wrong alignement char".to_owned()));
            }
            let (pos, val) = (
                (i as f64 * increment + start as f64) / 1000000.,
                u16_to_f64(d.value),
            );
            trace!("{} : {pos},{val}", i);

            csvdata.push((pos, val));
            writeln!(out, "{},{}", pos, val,)?;
        }
        // TODO: Dont return use mutex
        // info!("debug",format!("{} {}", data.len(), v.len());
        info!(
            "End at {end} expected {} and found {} ({:?})",
            b'}',
            v[end - 1],
            &v[end - 4..=end - 1]
        );
        Ok((name, csvdata))
    } else {
        Err(TinyError::ParseBeggining("{".to_owned()))
    }
}

/// Utility function converting the received data as u16 to float data
fn u16_to_f64(v: u16) -> f64 {
    (v as f64) / 32. - OFFSET as f64
}
