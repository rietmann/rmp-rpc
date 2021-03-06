extern crate env_logger;
extern crate futures;
extern crate rmp_rpc;
extern crate tokio_core;

mod client;
mod server;

use std::net::SocketAddr;

use futures::Future;
use rmp_rpc::serve;
use tokio_core::reactor::Core;

use client::Client;
use server::Calculator;

fn main() {
    env_logger::init().unwrap();
    let addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let mut reactor = Core::new().expect("Failed to start even loop");
    let handle = reactor.handle();

    reactor
        .handle()
        .spawn(serve(addr, Calculator::new(), handle));

    let client_future = Client::connect(&addr, &reactor.handle())
        .and_then(|client| {
            println!("client: connected");
            println!("client: add(1, 2, 3)");
            client
                .add(&[1, 2, 3])
                .and_then(|result| {
                    println!("client: result: {}", result);
                    Ok(client)
                })
                .or_else(|rpc_err| {
                    println!("client: add() failed: {}", rpc_err);
                    Err(rpc_err)
                })
        })
        .and_then(|client| {
            println!("client: sub(1)");
            client
                .sub(&[1])
                .and_then(|result| {
                    println!("client: result: {}", result);
                    Ok(client)
                })
                .or_else(|rpc_err| {
                    println!("sub() failed: {}", rpc_err);
                    Err(rpc_err)
                })
        })
        .and_then(|client| {
            println!("client: res()");
            client
                .res()
                .and_then(|result| {
                    println!("client: result: {}", result);
                    Ok(client)
                })
                .or_else(|rpc_err| {
                    println!("res() failed: {}", rpc_err);
                    Err(rpc_err)
                })
        })
        .and_then(|client| {
            println!("client: clear()");
            client
                .clear()
                .and_then(|result| {
                    println!("client: result: {}", result);
                    Ok(client)
                })
                .or_else(|rpc_err| {
                    println!("clear() failed: {}", rpc_err);
                    Err(rpc_err)
                })
        });
    let _ = reactor.run(client_future).unwrap();
}
