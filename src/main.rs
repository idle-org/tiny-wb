// use clap::Parser;
use memchr::memmem;
// use once_cell::sync::OnceCell;
use serialport::SerialPort;
use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::OnceLock,
    thread::sleep,
    time::Duration,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TinyError {
    #[error("couldn't parse data ({0})")]
    Parse(String),
    #[error("couldn't parse the beggining of the data ({0})")]
    ParseBeggining(String),
    #[error("couldn't parse the end of the data ({0})")]
    ParseEnd(String),
    #[error("Couldn't connect to the given port")]
    PortConnection(#[from] serialport::Error),
    #[error("Couldn't open the given file")]
    OpenOutput(#[from] std::io::Error),
}

// /// Simple program to greet a person
// #[derive(Parser, Debug)]
// #[command(version, about, long_about = None)]
// struct Args {
//     /// Name of the person to greet
//     #[arg(short, long)]
//     name: String,

//     /// Number of times to greet
//     #[arg(short, long, default_value_t = 1)]
//     count: u8,
// }

static EOF: OnceLock<memmem::Finder<'_>> = OnceLock::new();

fn main() -> Result<(), TinyError> {
    // let args = Args::parse();
    let ports = serialport::available_ports()?;
    for p in ports {
        println!("{}", p.port_name);
    }
    let mut port = serialport::new("/dev/ttyACM0", 115_200)
        .timeout(Duration::from_millis(2000))
        .open()
        .expect("Failed to open port");
    initialize(&mut port);
    println!("{}", send_cmd(&mut port, "version\r", 10));
    println!("{}", send_cmd(&mut port, "info\r", 10));
    println!("{}", send_cmd(&mut port, "threads\r", 10));

    // println!("{}", send_cmd(&mut port, "sweep 240000000 960000000 290\r",10));
    // println!("{}", send_cmd(&mut port, "frequencies\r",10));
    // println!("{}", send_cmd(&mut port, "sweep\r",10));
    // println!("{}", send_cmd(&mut port, "sweep start\r",10));
    // println!("{}", send_cmd(&mut port, "frequencies\r",10));
    println!("{}", send_cmd(&mut port, "repeat 2\r", 10));
    println!("{}", send_cmd(&mut port, "pause\r", 10));

    println!("{}", send_cmd(&mut port, "mode high input\r", 10));
    println!("{}", send_cmd(&mut port, "freq 935m\r", 10));
    println!("{}", send_cmd(&mut port, "scan 940 950m 10 101\r", 2000));
    initialize(&mut port);
    println!("{}", send_cmd(&mut port, "freq 937m\r", 10));
    println!("{:?}", scanraw(&mut port));
    println!("{}", send_cmd(&mut port, "resume\r", 10));
    wait_sync(200);
    Ok(())
}

fn initialize(port: &mut Box<dyn SerialPort>) {
    wait_sync(100);
    println!(
        "Initializing ... \nUninialized bytes: {:?}",
        read_port(port)
    );
    wait_sync(100)
}

/// Bad synchronisation method to ensure data is truthfully sent/received
fn wait_sync(duration: u64) {
    sleep(Duration::from_millis(duration));
}

static START: usize = 930;
static STOP: usize = 940;
static POINTS: usize = (STOP - START) * 100;
static INCREMENT: usize = 10; // 1000/100

/// Scan raw frequencies
fn scanraw(port: &mut Box<dyn SerialPort>) -> Result<(), TinyError> {
    unsafe { send_raw(port, &format!("scanraw {}m {}m {}\r", START, STOP, POINTS)) }
    // wait_sync();
    read_scanraw(port)?;
    wait_sync(100);
    Ok(())
}

/// Send a command and reads it to a string
fn send_cmd(port: &mut Box<dyn SerialPort>, command: &str, wait: u64) -> String {
    unsafe { send_raw(port, command) }
    wait_sync(wait);
    let r = read_utf8(port);
    wait_sync(200);
    r
}

/// Send a command without reading the output buffer
/// This command will produce unknown results if the output buffer is never read
unsafe fn send_raw(port: &mut Box<dyn SerialPort>, command: &str) {
    let output = command.as_bytes();
    println!("Sending: {}", command);
    port.write(output).expect("Write failed!");
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
            res.extend_from_slice(&mut serial_buf[..pos]);
            break;
        }
        res.extend_from_slice(&mut serial_buf[..r]);
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
    let mut out = BufWriter::new(File::create("test.csv")?);
    if let Some(begining) = memchr::memchr(b'{', &v) {
        // Finds the } marking the end
        if let Some(end) = memchr::memchr(b'}', &v) {
            let data = &v[begining + 1..end];
            for (i, slice) in data.chunks_exact(3).enumerate() {
                let (x, msb, lsb) = (slice[0], slice[1], slice[2]);
                // println!("{} {} {} -> {}", x, msb, lsb, bytes_to_f64(msb, lsb));
                // println!("{} -> {}", i * INCREMENT, bytes_to_f64(msb, lsb));
                writeln!(
                    out,
                    "{},{}",
                    (i * INCREMENT + START * 1000) as f64 / 1000.,
                    bytes_to_f64(msb, lsb)
                )?;
            }
            // println!("Slice: {}:{}, Data:{:?}", begining, end, data);
            Ok(())
        } else {
            Err(TinyError::ParseEnd("}".to_owned()))
        }
    } else {
        Err(TinyError::ParseBeggining("{".to_owned()))
    }
}

fn bytes_to_f64(msb: u8, lsb: u8) -> f64 {
    let a = (((lsb as u16) << 8) | msb as u16);
    a as f64 / 32. - 128.
}

/// The result of a scan
pub struct ScanResult {
    start: usize,
    stop: usize,
    points: usize,
    status: ScanStatus,
    data: Vec<f64>,
}

pub enum ScanStatus {
    Success,
    Failure,
    Interrupted,
}
// First line is discardable as it contains only a repead of sent command
// Read needs to stop on "ch>" 99, 104, 62
// Need to send resume afterward
// Doc: https://tinysa.org/wiki/pmwiki.php?n=Main.USBInterface
// Working example: http://athome.kaashoek.com/tinySA/python/
// Firmwares: All firmware files are located in http://athome.kaashoek.com/tinySA/DFU/
// QT5 GUI:https://github.com/g4ixt/QtTinySA?tab=readme-ov-file
// TUI: Rattatui
// ProgressB: Indicatif
// Version: tinySA4: Ultra, tinySA_ Tiny
// Scan will start with 123 and end with 125
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
//     // println!("{}", send_cmd(&mut port, "scanraw 240m 960m 1000\r"));
//     send_cmd(&mut port, "resume\r");
// }
