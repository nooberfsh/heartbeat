#![feature(conservative_impl_trait)]

extern crate tokio_core;
extern crate tokio_io;
extern crate futures;
extern crate bytes;

use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::collections::HashSet;

use futures::future::{loop_fn, Loop};
use futures::{Future, Stream, Sink};
use tokio_core::net::{TcpListener, TcpStream};
use tokio_core::reactor::Core;
use tokio_io::codec::length_delimited::Framed;
use tokio_io::AsyncRead;
use bytes::BytesMut;

type Connections = Rc<RefCell<HashSet<SocketAddr>>>;

struct Server {
    core: Core,
    conns: Connections,
}

struct Connection {
    addr: SocketAddr,
    sock: Framed<TcpStream>,
    bytes: Option<BytesMut>,
}

impl Connection {
    fn new(addr: SocketAddr, sock: Framed<TcpStream>) -> Self {
        Connection {
            addr: addr,
            sock: sock,
            bytes: None,
        }
    }

    fn recv(self) -> impl Future<Item = Self, Error = io::Error> {
        let addr = self.addr;
        self.sock.into_future().map_err(|(e, _)| e).and_then(
            move |(bytes,
                   sock)| {
                let mut conn = Connection::new(addr, sock);
                conn.bytes = bytes;
                if conn.bytes.is_none() {
                    Err(io::Error::new(io::ErrorKind::Other, "closed by client"))
                } else {
                    Ok(conn)
                }
            },
        )
    }

    fn send(mut self) -> impl Future<Item = Self, Error = io::Error> {
        let addr = self.addr;
        let bytes = self.bytes.take().unwrap();
        self.sock.send(bytes).map(move |s| Connection::new(addr, s))
    }
}


impl Server {
    fn new() -> Self {
        Server {
            core: Core::new().unwrap(),
            conns: Connections::default(),
        }
    }

    fn run(&mut self, addr: SocketAddr) {
        let handle = self.core.handle();

        // Bind the server's socket
        let listener = TcpListener::bind(&addr, &handle).unwrap();

        let conns = Rc::clone(&self.conns);
        let server = listener.incoming().for_each(move |(sock, addr)| {
            conns.borrow_mut().insert(addr);
            let conn = Connection::new(addr, Framed::new(sock));
            let f = |conn: Connection| {
                conn.recv().and_then(|conn| conn.send()).and_then(|conn| {
                    Ok(Loop::Continue(conn))
                })
            };
            let conns = Rc::clone(&conns);
            loop_fn(conn, f).then(move |r: Result<(), io::Error>| {
                assert!(r.is_err());
                let _ = conns.borrow_mut().remove(&addr);
                Ok(())
            })
        });

        // Spin up the server on the event loop
        self.core.run(server).unwrap();
    }

    fn conns_num(&self) -> usize {
        self.conns.borrow().len()
    }
}

fn main() {
    println!("Hello, world!");
    let addr = "127.0.0.1:8799".parse().unwrap();
    let mut s = Server::new();
    s.run(addr);
    println!("finish running");
}
