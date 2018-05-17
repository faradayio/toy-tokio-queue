//! A tiny async echo server with Tokio. This code is taken directly from
//! https://tokio.rs/

extern crate bytes;
extern crate failure;
extern crate futures;
extern crate openssl;
#[macro_use]
extern crate structopt;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_openssl;

use failure::Error;
use std::time::{Duration, Instant};
use structopt::StructOpt;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::timer::Interval;
use tokio_io::codec::LinesCodec;

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
    let addr = ADDR.parse()?;

    let client = TcpStream::connect(&addr)
        .and_then(|tcp| {
            // Convert the raw socket into a line-oriented stream, and split
            // the read and write halves.
            let framed = tcp.framed(LinesCodec::new());
            let (sink, stream) = framed.split();

            // Print out the messages that we receive.
            stream.fold(sink, |sink, line| {
                println!("FROM SERVER: {:?}", line);
                sink.send("ACK".into())
            })
        })
        .map(|_| {
            println!("Done!");
        })
        .map_err(|err| {
            eprintln!("client error: {:?}", err);
        });

    tokio::run(client);

    Ok(())
}
