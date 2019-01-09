// Adapted from this example: https://riptutorial.com/rust/example/4404/a-simple-tcp-client-and-server-application--echo

use clap::{App, Arg};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("TCP Echo Server")
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .takes_value(true)
                .required(true)
                .help("Port to bind to"),
        )
        .get_matches();

    let port = matches.value_of("port").unwrap();

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
    // accept connections and process them, spawning a new thread for each one
    println!("Server listening on port {}", port);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr()?);
                thread::spawn(move || handle_client(stream));
            }
            Err(e) => Err(e)?,
        }
    }
    // close the socket server
    drop(listener);

    Ok(())
}

fn handle_client(mut stream: TcpStream) -> Result<(), std::io::Error> {
    let mut data = [0 as u8; 1024];
    while match stream.read(&mut data) {
        Ok(size) if size == 0 => {
            println!("Socket closed by {}", stream.peer_addr().unwrap());
            stream.shutdown(Shutdown::Both)?;
            false
        }
        Ok(size) => {
            stream.write_all(&data[0..size])?;
            true
        }
        Err(_) => {
            println!(
                "An error occurred, terminating connection with {}",
                stream.peer_addr().unwrap()
            );
            stream.shutdown(Shutdown::Both)?;
            false
        }
    } {}

    Ok(())
}
