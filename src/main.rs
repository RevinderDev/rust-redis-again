#![allow(unused_imports)]
use bytes::{Buf, BytesMut};
use commands::Command;
use db::Database;
use parser::{ParserError, RespValue};
use std::{
    borrow::BorrowMut,
    collections::HashMap,
    io::{BufRead, BufReader, Read, Result, Write},
    str,
    sync::{Arc, Mutex},
    thread,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod commands;
mod db;
mod parser;

async fn handle_connection(mut socket: TcpStream, db: Database) {
    let mut buffer = BytesMut::with_capacity(4096);

    loop {
        loop {
            println!("Current buffer: {buffer:#?}");
            let response = match RespValue::parse(&buffer) {
                Ok((value, consumed)) => {
                    let command_result = Command::from_resp(value);

                    let response = match command_result {
                        Ok(command) => command.execute(&db),
                        Err(e) => format!("-ERR {}\r\n", e).into_bytes(),
                    };
                    buffer.advance(consumed);
                    response
                }
                Err(ParserError::Incomplete) => break,
                Err(ParserError::InvalidFormat(e)) => {
                    let _ = socket.write_all(format!("-ERR {}\r\n", e).as_bytes()).await;
                    // NOTE: Do you want to close connection here?
                    return;
                }
            };

            if let Err(e) = socket.write_all(&response).await {
                eprintln!("failed to write response: {:?}", e);
                return;
            }
        }

        match socket.read_buf(&mut buffer).await {
            Ok(0) => {
                println!("Client closed connection");
                return;
            }
            Ok(n) => {
                println!("Read {} bytes from socket", n);
            }
            Err(e) => {
                eprintln!("failed to read from socket; err = {:?}", e);
                return;
            }
        }
    }
}

async fn server_loop() {
    let db: Database = Arc::new(Mutex::new(HashMap::new()));
    let listener = match TcpListener::bind("127.0.0.1:6379").await {
        Ok(s) => s,
        Err(e) => {
            println!("Error unable to start the server: {e}");
            return;
        }
    };

    loop {
        match listener.accept().await {
            Ok((socket, _)) => {
                let db_clone = db.clone();
                tokio::spawn(async move {
                    handle_connection(socket, db_clone).await;
                });
            }
            Err(e) => eprintln!("Failed to establish connectin: {:?}", e),
        };
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    server_loop().await;
    Ok(())
}
