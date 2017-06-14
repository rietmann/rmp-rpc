//! Building blocks for building msgpack-rpc servers.
use std::io;
use std::net::SocketAddr;
use std::collections::HashMap;
use tokio_core::net::{TcpStream, TcpListener};
use tokio_core::reactor::Core;
use tokio_io::AsyncRead;
use tokio_io::codec::Framed;
use futures::{Async, Poll, Future, Stream, Sink};
use message::{Response, Request, Notification, Message};
use std::error::Error;
use codec::Codec;

pub trait Service {
    type Error: Error;

    fn handle_request(
        &mut self,
        request: &Request,
    ) -> Box<Future<Item = Result<Response, Self::Error>, Error = io::Error>>;

    fn handle_notification(
        &mut self,
        notification: &Notification,
    ) -> Box<Future<Item = Result<(), Self::Error>, Error = io::Error>>;
}

pub trait ServiceBuilder {
    type Service: Service + 'static;

    fn build(&self) -> Self::Service;
}

pub struct Protocol<S: Service> {
    service: S,
    done: bool,
    stream: Framed<TcpStream, Codec>,
    request_tasks: HashMap<u32, Box<Future<Item = Result<Response, S::Error>, Error = io::Error>>>,
    notification_tasks: Vec<Box<Future<Item = Result<(), S::Error>, Error = io::Error>>>,
}

impl<S> Protocol<S>
where
    S: Service + 'static,
{
    fn new(service: S, tcp_stream: TcpStream) -> Self {
        Protocol {
            service: service,
            done: false,
            stream: tcp_stream.framed(Codec),
            request_tasks: HashMap::new(),
            notification_tasks: Vec::new(),
        }
    }

    fn handle_msg(&mut self, msg: Message) {
        match msg {
            Message::Request(request) => {
                let response = self.service.handle_request(&request);
                self.request_tasks.insert(request.id, response);
            }
            Message::Notification(notification) => {
                let outcome = self.service.handle_notification(&notification);
                self.notification_tasks.push(outcome);
            }
            Message::Response(_) => {
                return;
            }
        }
    }

    fn process_notifications(&mut self) {
        let mut done = vec![];
        for (idx, task) in self.notification_tasks.iter_mut().enumerate() {
            match task.poll().unwrap() {
                Async::Ready(Ok(())) => done.push(idx),
                Async::Ready(Err(e)) => panic!("{}", e),
                Async::NotReady => continue,
            }
        }
        for idx in done.iter().rev() {
            self.notification_tasks.remove(*idx);
        }
    }

    fn process_requests(&mut self) {
        let mut done = vec![];
        for (id, task) in &mut self.request_tasks {
            match task.poll().unwrap() {
                Async::Ready(Ok(mut response)) => {
                    response.id = *id;
                    done.push(*id);
                    if !self.stream
                        .start_send(Message::Response(response))
                        .unwrap()
                        .is_ready()
                    {
                        panic!("the sink is full")
                    }
                }
                Async::Ready(Err(e)) => panic!("{}", e),
                Async::NotReady => continue,
            }
        }

        for idx in done.iter_mut().rev() {
            let _ = self.request_tasks.remove(idx);
        }
    }

    fn flush(&mut self) {
        if self.stream.poll_complete().unwrap().is_ready() {
            self.done = true;
        } else {
            self.done = false;
        }
    }
}

impl<S> Future for Protocol<S>
where
    S: Service + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // TODO: graceful shutdown (ie return Async::Ready)
        loop {
            match self.stream.poll().unwrap() {
                Async::Ready(Some(msg)) => self.handle_msg(msg),
                Async::Ready(None) => panic!("shutdown is not implemented"),
                Async::NotReady => break,
            }
        }
        self.process_notifications();
        self.process_requests();
        self.flush();
        Ok(Async::NotReady)
    }
}


pub fn serve<B: ServiceBuilder>(address: &SocketAddr, service_builder: B) {
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let listener = TcpListener::bind(address, &handle).unwrap();
    core.run(listener.incoming().for_each(|(stream, _address)| {
        let service = service_builder.build();
        let proto = Protocol::new(service, stream);
        handle.spawn(proto.map_err(|_| ()));
        Ok(())
    })).unwrap()
}
