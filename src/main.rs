#![allow(unused_imports)]
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    str, thread,
};

fn handle_connection(mut stream: TcpStream) {
    println!("New thread started for a connection.");
    let mut reader = BufReader::new(&mut stream);
    let mut buffer = String::new();

    loop {
        buffer.clear();

        let _bytes_read = match reader.read_line(&mut buffer) {
            Ok(0) => {
                println!("Client disconnected.");
                return; // Exit the function, which ends the thread
            }
            Ok(n) => n,
            Err(e) => {
                println!("Failed to read from stream: {}", e);
                return;
            }
        };

        println!("Received: {}", buffer);
        if buffer.trim().to_uppercase().starts_with("PING") {
            let stream_writer = reader.get_mut();
            stream_writer.write_all(b"+PONG\r\n").unwrap();
        }
    }
}

fn main() {
    println!("Logs from your program will appear here!");
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(mut _stream) => {
                thread::spawn(move || {
                    handle_connection(_stream);
                });
                // println!("accepted new connection");
                // let mut reader = BufReader::new(&mut _stream);
                // let mut buffer = String::new();
                //
                // loop {
                //     buffer.clear();
                //     let bytes_read = reader.read_line(&mut buffer).unwrap();
                //     if bytes_read == 0 {
                //         println!("client disconnected");
                //         break;
                //     }
                //     println!("Read the buffer: {}", buffer.trim());
                //     if buffer.contains("PING") {
                //         let stream_writer = reader.get_mut();
                //         stream_writer.write_all(b"+PONG\r\n").unwrap();
                //         println!("Responded with PONG");
                //         break;
                //     }
                // }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
