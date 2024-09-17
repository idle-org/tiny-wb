use clap::Parser;
use serialport::SerialPort;
use std::time::Duration;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    name: String,

    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,
}

fn main() {
    let args = Args::parse();
    let ports = serialport::available_ports().expect("No ports found!");
    for p in ports {
        println!("{}", p.port_name);
    }
    let mut port = serialport::new("/dev/ttyACM0", 115_200)
        .timeout(Duration::from_millis(2000))
        .open()
        .expect("Failed to open port");
    // println!("{}", send_cmd(&mut port, "0\r"));
    // println!("{}", send_cmd(&mut port, "version\r"));
    println!("{}", send_cmd(&mut port, "scan 240m 960m 10 11\r"));
    send_cmd(&mut port, "resume\r");
}

fn send_cmd(port: &mut Box<dyn SerialPort>, command: &str) -> String {
    // let command = &"0";
    let output = command.as_bytes();
    port.write(output).expect("Write failed!");

    let mut serial_buf: Vec<u8> = vec![0; 256];
    // let res = String::new();
    let mut res = Vec::new();
    let mut tsi = 0;
    while let Ok(r) = port.read(serial_buf.as_mut_slice()) {
        println!("{} {:?}", r, serial_buf);
        tsi += r;
        res.append(&mut serial_buf.clone());
    }
    println!("{} {}", tsi, String::from_utf8(res).expect("Can't convert"));
    String::from_utf8(serial_buf).expect("Converting to str")
}

// First line is discardable as it contains only a repead of sent command
// Read needs to stop on "ch>"
// Need to send resume afterward
// Doc: https://tinysa.org/wiki/pmwiki.php?n=Main.USBInterface
// Working example: http://athome.kaashoek.com/tinySA/python/
// Firmwares: All firmware files are located in http://athome.kaashoek.com/tinySA/DFU/
// QT5 GUI:https://github.com/g4ixt/QtTinySA?tab=readme-ov-file
// TUI: Rattatui
// ProgressB: Indicatif
