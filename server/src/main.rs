#![feature(try_from)]

use std::convert::TryInto;

use futures::{
    future::{Either, Loop},
};
use actix_service::{NewService, IntoNewService};
use tokio::{io, net::TcpStream, prelude::*};

fn read_u64(stream: TcpStream) -> impl Future<Item=(TcpStream, u64), Error=io::Error> {
    tokio::io::read_exact(stream, [0u8; 8])
        .map(|(stream, buf)| {
            (stream, u64::from_be_bytes(buf))
        })
}

fn handle_ping(stream: TcpStream) -> impl Future<Item=TcpStream, Error=io::Error> {
    read_u64(stream)
        .and_then(|(stream, id)| {
            println!("received ping from {}", id);
            tokio::io::write_all(stream, [1])
        })
        .map(|(stream, _)| stream)
}

fn handle_message(stream: TcpStream) -> impl Future<Item=TcpStream, Error=io::Error> {
    tokio::io::read_exact(stream, [0u8; 16])
        .map(|(stream, buf)| {
            (stream, u64::from_be_bytes(buf[0..8].try_into().unwrap()), u64::from_be_bytes(buf[8..16].try_into().unwrap()))
        })
        .and_then(|(stream, id, len)| {
            tokio::io::read_exact(stream, vec![0; len as usize])
                .map(move |(stream, buf)| (stream, id, buf))
        })
        .map(|(stream, id, msg)| {
            print!("{} says \"", id);
            std::io::stdout().write_all(&msg).unwrap();
            println!("\"");
            stream
        })
}

fn handle_broadcast(stream: TcpStream) -> impl Future<Item=TcpStream, Error=io::Error> {
    tokio::io::read_exact(stream, [0u8; 16])
        .map(|(stream, buf)| {
            (stream, u64::from_be_bytes(buf[0..8].try_into().unwrap()), u64::from_be_bytes(buf[8..16].try_into().unwrap()))
        })
        .and_then(|(stream, id, len)| {
            tokio::io::read_exact(stream, vec![0; len as usize])
                .map(move |(stream, buf)| (stream, id, buf))
        })
        .map(|(stream, id, msg)| {
            print!("{} broadcasts \"", id);
            std::io::stdout().write_all(&msg).unwrap();
            println!("\"");
            stream
        })
}

fn handle_frame(stream: TcpStream) -> impl Future<Item=Loop<(), TcpStream>, Error=io::Error> {
    io::read_exact(stream, [0u8])
        .and_then(|(stream, buf)| -> Either<Box<dyn Future<Item=_,Error=_>>, _> {
            match buf[0] {
                1 => Either::A(Box::new(handle_ping(stream).map(|stream| Loop::Continue(stream)))),
                2 => Either::A(Box::new(handle_message(stream).map(|stream| Loop::Continue(stream)))),
                c => {
                    eprintln!("Unrecognized code {}, closing connection with {}", c, stream.peer_addr().unwrap());
                    Either::B(future::ok(Loop::Break(())))
                }
            }
        })
}

fn main() {
    let sys = actix_rt::System::new("test");

    actix_server::build()
        .bind(
            // configure service pipeline
            "basic",
            "0.0.0.0:2000",
            || {
                (move |stream: tokio::net::TcpStream| {
                    let peer_addr = stream.peer_addr().unwrap();
                    println!("incoming connection from {}", peer_addr);
                    future::loop_fn(stream, handle_frame)
                        .map_err(move |e| (e, peer_addr))
                })
                .into_new_service()
                .map_err(|(e, peer_addr)| {
                    match e.kind() {
                        io::ErrorKind::UnexpectedEof => println!("connection with {} closed by client", peer_addr),
                        _ => println!("{}", e),
                    }
                })
                .map(|_| ())
            },
        )
        .unwrap()
        .start();

    sys.run();
}
