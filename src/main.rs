//! A tiny async echo server with Tokio. This code is taken directly from
//! https://tokio.rs/

extern crate bytes;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate openssl;
#[macro_use]
extern crate structopt;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_openssl;

mod connection;

use failure::Error;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::timer::Interval;
use tokio_io::codec::LinesCodec;

use connection::Connection;

const ADDR: &'static str = "127.0.0.1:12345";

/// Use `structopt` to declare our command-line arguments.
#[derive(Debug, StructOpt)]
#[structopt(name = "toy-tokio-queue")]
enum Opt {
    #[structopt(name = "server")]
    Server,
    #[structopt(name = "client")]
    Client,
}

/// Parse our command-line arguments and dispatch.
fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    match opt {
        Opt::Server => server(),
        Opt::Client => client(),
    }
}

/// Our server.
fn server() -> Result<(), Error> {
    // Bind the server's socket
    let addr = ADDR.parse()?;
    let tcp = TcpListener::bind(&addr)?;

    // Iterate incoming connections
    let server = tcp.incoming().for_each(|tcp| {
        // Manipulate one line at a time.
        let framed = tcp.framed(LinesCodec::new());

        // Split up the read and write halves
        let (sink, stream) = framed.split();

        // Send messages periodically.
        let messages = Interval::new(Instant::now(), Duration::from_secs(2))
            .map(|inst| format!("message at {:?}", inst))
            .map_err(|err| -> io::Error {
                io::Error::new(io::ErrorKind::Other, format!("{}", err))
            })
            .forward(sink)
            .map(|_| {})
            .map_err(|err| eprintln!("ERROR: {}", err));
        tokio::spawn(messages);

        // Print messages sent by the client.
        let conn = stream
            .for_each(|message| {
                println!("FROM CLIENT: {:?}", message);
                Ok(())
            })
            // print what happened
            .map(|_| {
                println!("echoed");
            })
            // Handle any errors
            .map_err(|err| {
                eprintln!("IO error {:?}", err)
            });
        tokio::spawn(conn);

        Ok(())
    })
    .map_err(|err| {
        eprintln!("server error {:?}", err);
    });

    // Start the runtime and spin up the server
    tokio::run(server);

    Ok(())
}

/// Our client.
fn client() -> Result<(), Error> {

        let conn = Connection::open("127.0.0.1", 12345)?;
        let (mut read_conn, write_conn) = conn.split();
        loop {
            let line = read_conn.read()?;
            let mut write_conn = write_conn.clone();
            ::std::thread::spawn(move || {
                println!("FROM SERVER: {:?}", line);
                write_conn.write("ACK".into())
                    .expect("could not ACK message");
            });
        }

    Ok(())
}
