//! The "lib" used for all scan related functions

/// The default start value for a scan
static START: usize = 240000000;
/// The default stop value for a scan
static STOP: usize = 959000000;
/// The default number of points
static POINTS: usize = 290; //(STOP - START) * 10;
/// The default increment (deduced from the number of points)
static INCREMENT: usize = (STOP - START) / POINTS; // 1000/100
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
    sync::OnceLock,
    thread::sleep,
    time::Duration,
};

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
pub(crate) fn scan() -> Result<(), TinyError> {
    let ports = serialport::available_ports()?;
    for p in ports {
        println!("{}", p.port_name);
    }
    let mut port = serialport::new("/dev/ttyACM0", 115_200)
        .timeout(Duration::from_millis(3000))
        .open()
        .expect("Failed to open port");
    initialize(&mut port);
    println!("{}", send_cmd(&mut port, "help\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "version\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "info\r", SHORT_WAIT));

    println!(
        "{}",
        send_cmd(&mut port, &format!("zero {}\r", OFFSET), SHORT_WAIT)
    );
    println!("{}", send_cmd(&mut port, "leveloffset\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "deviceid\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "vbat offset\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "vbat\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "usart_cfg\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "marker\r", SHORT_WAIT));

    println!("{}", send_cmd(&mut port, "sweep\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "mode\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "lna\r", SHORT_WAIT));

    println!("{}", send_cmd(&mut port, "pause\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "vbat\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "marker\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "sweep\r", SHORT_WAIT));
    println!("{}", send_cmd(&mut port, "mode high input\r", SHORT_WAIT));

    println!("{}", send_cmd(&mut port, "sweep\r", SHORT_WAIT));
    println!(
        "{}",
        send_cmd(&mut port, &format!("sweep start {}\r", START), SHORT_WAIT)
    );
    println!(
        "{}",
        send_cmd(&mut port, &format!("sweep stop {}\r", STOP), SHORT_WAIT)
    );
    println!("{}", send_cmd(&mut port, "sweep\r", SHORT_WAIT));
    println!(
        "{}",
        send_cmd(&mut port, "scan 240000000 959000000 290 101\r", TIMEOUT)
    );
    // println!(
    //     "{}",
    //     send_cmd(&mut port, &format!("scan {} {} 100 101\r", START, STOP), 50)
    // );
    println!("{:?}", scanraw(&mut port));
    println!("{}", send_cmd(&mut port, "resume\r", SHORT_WAIT));
    wait_sync(LONG_WAIT);
    Ok(())
}

/// A (lame) function used for initialisation:
/// Wait's for a whole timeout for data stuck in the buffer and dumps it
fn initialize(port: &mut Box<dyn SerialPort>) {
    wait_sync(SHORT_WAIT);
    println!(
        "Initializing ... \nUninialized bytes: {:?}",
        read_port(port)
    );
    wait_sync(SHORT_WAIT)
}

/// Bad synchronisation method to ensure data is truthfully sent/received
fn wait_sync(duration: u64) {
    sleep(Duration::from_millis(duration));
}

/// Scan raw frequencies
fn scanraw(port: &mut Box<dyn SerialPort>) -> Result<(), TinyError> {
    // let mut f = File::open("b.txt")?;
    // let mut s = String::new();
    // let d = f.read_to_string(&mut s)?;
    // println!("{}", s);
    // let v = hex::decode(&s[..s.len() - 1]).?();
    // println!("{:?}", v);
    // parse_data(v)?;
    unsafe { send_raw(port, &format!("scanraw {} {} {}\r", START, STOP, POINTS)) }
    wait_sync(SHORT_WAIT);
    read_scanraw(port)?;
    wait_sync(LONG_WAIT);
    Ok(())
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
    println!("Sending: {}", command);
    let _r = port.write(output).expect("Write failed!");
}

/// Reads from port and tries to convert it to string with loss
fn read_utf8(port: &mut Box<dyn SerialPort>) -> String {
    String::from_utf8_lossy(&read_port(port)).to_string()
}

/// Reads the serial port until it reaches EOF
fn read_port(port: &mut Box<dyn SerialPort>) -> Vec<u8> {
    let mut serial_buf: Vec<u8> = vec![0; 256];
    let mut res = Vec::new();
    let finder = EOF.get_or_init(|| memmem::Finder::new("ch>"));
    while let Ok(r) = port.read(serial_buf.as_mut_slice()) {
        if let Some(pos) = finder.find(&serial_buf) {
            // println!("{:?}", serial_buf);
            res.extend_from_slice(&serial_buf[..pos]);
            break;
        }
        res.extend_from_slice(&serial_buf[..r]);
    }
    // println!("{:?}", res);
    res
}

/// Scan for a raw frequencies, assumes everything is setup and the scan can proceed
/// Waits the normal start Character '{' and begins treating data as normal
///
/// Returns a struct summerizing the information processed
fn read_scanraw(port: &mut Box<dyn SerialPort>) -> Result<(), TinyError> {
    let v = read_port(port);
    // Find the { marking the beggining
    parse_data(v)
}

/// A utility function used for parsing the raw data of a scan
pub fn parse_data(v: Vec<u8>) -> Result<(), TinyError> {
    let mut values: Vec<u16> = Vec::with_capacity(POINTS);
    let mut out = BufWriter::new(File::create("test.csv")?);
    if let Some(begining) = memchr::memchr(b'{', &v) {
        // Finds the } marking the end
        if let Some(end) = memchr::memchr(b'}', &v) {
            let data = &v[begining + 1..end];
            for (i, slice) in data.chunks_exact(3).enumerate() {
                let s: &[u8; 3] = &[slice[0], slice[1], slice[2]];
                let d = DataPoint::unpack(s)?;
                // println!(
                //     "{} {} {} -> {} : {}",
                //     x,
                //     b1,
                //     b2,
                //     d.value,
                //     u16_to_f64(d.value)
                // );
                values.push(d.value);
                writeln!(
                    out,
                    "{},{}",
                    (i * INCREMENT + START) as f64 / 1000000.,
                    u16_to_f64(d.value) // bytes_to_f64(b1, b2)
                )?;
            }
            println!(
                "Found {} values, min: {:?}, max: {:?}",
                values.len(),
                u16_to_f64(*values.iter().min().unwrap_or(&0)),
                u16_to_f64(*values.iter().max().unwrap_or(&0)),
            );
            Ok(())
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

// /// The result of a scan
// pub struct ScanResult {
//     start: usize,
//     stop: usize,
//     points: usize,
//     status: ScanStatus,
//     data: Vec<f64>,
// }

// pub enum ScanStatus {
//     Success,
//     Failure,
//     Interrupted,
// }
// loop {
//     // println!("{}", send_cmd(&mut port, "0\r"));
//     print!("ch> ");
//     io::stdout().flush().expect("IO error");
//     let mut input = String::new();
//     match io::stdin().read_line(&mut input) {
//         Ok(_) => {
//             println!("{}", send_cmd(&mut port, &format!("{}\r", input)));
//         }
//         Err(e) => println!("Error {}", e),
//     }
//     // println!("{}", send_cmd(&mut port, "scan 240m 960m 240 11\r"));
//     // println!("{}", send_cmd(&mut port, "scanraw 240m 960m 10000\r"));
// //     // println!("{}", send_cmd(&mut port, "scanraw 240m 960m 1000\r"));
// //     send_cmd(&mut port, "resume\r");
// }
