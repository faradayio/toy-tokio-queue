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
use futures::future::ok;
use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
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

        // Copy the data back to the client
        let conn = stream.forward(sink)
            // print what happened
            .map(|_| {
                println!("echoed");
            })
            // Handle any errors
            .map_err(|err| {
                println!("IO error {:?}", err)
            });

        // Spawn the future as a concurrent task
        tokio::spawn(conn);

        Ok(())
    })
    .map_err(|err| {
        println!("server error {:?}", err);
    });

    // Start the runtime and spin up the server
    tokio::run(server);

    Ok(())
}

/// Our client.
fn client() -> Result<(), Error> {
    let addr = ADDR.parse()?;

    let client = TcpStream::connect(&addr)
        .and_then(move |tcp| {
            let framed = tcp.framed(LinesCodec::new());
            let (sink, stream) = framed.split();
            (sink.send("Hello!".to_owned()), ok(stream)).into_future()
        })
        .and_then(move |(_sink, stream)| {
            eprintln!("(receiving)");
            stream.for_each(|line| {
                println!("{}", line);
                Ok(())
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
