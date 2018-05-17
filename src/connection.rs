use failure;
use futures::{Future, Sink, Stream, sync::mpsc};
use openssl::ssl::{SslConnector, SslMethod};
use std::net::{SocketAddr, ToSocketAddrs};
use tokio;
use tokio::io;
use tokio::net::TcpStream;
use tokio_io::{AsyncRead, AsyncWrite, codec::LinesCodec};
use tokio_openssl::{SslConnectorExt, SslStream};

/// For this toy implementation, our `Frame`s are just single lines of text.
type Frame = String;

/// For this toy implementation, just use `failure::Error`.
type AMQPError = failure::Error;

/// Our result type.
type AMQPResult<T> = Result<T, failure::Error>;

/// A connection to an AMQP server.
pub struct Connection {
    /// A readable stream of `Frame`s, boxed so that we don't have know exactly
    /// how it's implemented and we can treat it as an abstract interface.
    stream: Box<Stream<Item = Frame, Error = io::Error> + Send + Sync>,

    /// A writable sink for `Frame`s.
    sink: Box<Sink<SinkItem = Frame, SinkError = io::Error> + Send + Sync>,
}

impl Connection {
    /// Open a TLS connection to the specified address.
    #[cfg(feature = "tls")]
    pub fn open_tls(host: &str, port: u16) -> AMQPResult<Connection> {
        let addr = socket_addr(host, port)?;
        TcpStream::connect(&addr)
            .and_then(|tcp: TcpStream| {
                SslConnector::builder(SslMethod::tls())
                    .expect("could not create builder")
                    .build()
                    .connect_async(host, tcp)
                    .map_err(|err| {
                        io::Error::new(io::ErrorKind::Other, err)
                    })
            })
            // TODO: Send AMQP protocol magic, maybe in codec.
            .wait()
            .map(|tls| {
                // Break into frames and split now, because this is much easier
                // before we stick this in a `Box` and lose type information.
                let (sink, stream) = tls.framed(LinesCodec::new()).split();
                Connection {
                    sink: Box::new(sink),
                    stream: Box::new(stream),
                }
            })
            .map_err(|err| format_err!("could not connect: {}", err))
    }

    /// Open a regular TCP connection to the specified address.
    pub fn open(host: &str, port: u16) -> AMQPResult<Connection> {
        let addr = socket_addr(host, port)?;
        // TODO: Send AMQP protocol magic, maybe in codec.
        TcpStream::connect(&addr).wait()
            .map(|tcp| {
                // Break into frames and split now, because this is much easier
                // before we stick this in a `Box` and lose type information.
                let (sink, stream) = tcp.framed(LinesCodec::new()).split();
                Connection {
                    sink: Box::new(sink),
                    stream: Box::new(stream),
                }
            })
            .map_err(|err| format_err!("could not connect: {}", err))
    }

    /// Split this connection into an independent `(ReadConnection,
    /// WriteConnection)` pair.
    pub fn split(self) -> (ReadConnection, WriteConnection) {
        // Consume our `self`, and extract our `sink` and `stream`.
        let (sink, stream) = (self.sink, self.stream);

        // Set up our `ReadConnection`.
        let (read_sender, read_receiver) = mpsc::channel(0);
        let read_conn = ReadConnection {
            receiver: Some(read_receiver),
        };

        // Copy inbound frames from `stream` to `read_sender`.
        let reader = stream
            .map_err(|err| -> AMQPError { err.into() })
            .forward(read_sender)
            .map(|_| { eprintln!("reader done") })
            .map_err(|e| { eprintln!("reader failed") });
        tokio::spawn(reader);

        // Set up our `WriteConnection`.
        let (write_sender, write_receiver) = mpsc::channel(0);
        let write_conn = WriteConnection {
            frame_max_limit: 131072,
            sender: Some(write_sender),
        };

        // Copy outbound frames from `write_receiver` to `sink`.
        let writer = write_receiver
            .map_err(|()| { format_err!("all writers gone") })
            .forward(sink)
            .map(|_| { eprintln!("writer done") })
            .map_err(|e| { eprintln!("writer failed: {}", e) });
        tokio::spawn(writer);

        (read_conn, write_conn)
    }
}

/// Convert a hostname and port into an IP address
fn socket_addr(host: &str, port: u16) -> AMQPResult<SocketAddr> {
    (host, port).to_socket_addrs()?
        .next()
        .ok_or_else(|| {
            format_err!("could not look up addr")
        })
}

/// A connection which can read frames from an AMQP server.
pub struct ReadConnection {
    receiver: Option<mpsc::Receiver<Frame>>,
}

impl ReadConnection {
    /// Read the next frame. Blocking.
    pub fn read(&mut self) -> AMQPResult<Frame> {
        // Take ownership of the `receiver` so we can pass it to `into_future`.
        // This will fail if a previous `read` failed.
        let receiver = self.receiver.take().ok_or_else(|| {
            format_err!("tried to receive, but there's no sender")
        })?;

        // Use `into_future` to wait for the next item received on our stream.
        // This returns the next value in stream, as well as `rest`, which
        // is a stream that will return any following values.
        match receiver.into_future().wait() {
            // We received a value normally, so replace `self.receiver` with
            // our new `receiver`
            Ok((Some(frame), rest)) => {
                self.receiver = Some(rest);
                Ok(frame)
            }
            Ok((None, _rest)) => {
                Err(format_err!("end of stream (no more data)"))
            }
            Err(((), _rest)) => {
                Err(format_err!("end of stream (sender dropped)"))
            }
        }
    }
}

/// A connection which can write frames to an AMQP server.
pub struct WriteConnection {
    frame_max_limit: u32,
    sender: Option<mpsc::Sender<Frame>>,
}

impl WriteConnection {
    /// Set the maximum size of `BODY` frame to send as a single chunk. Larger
    /// frames will be broken into pieces.
    pub fn set_frame_max_limit(&mut self, frame_max_limit: u32) {
        self.frame_max_limit = frame_max_limit;
    }

    /// Write a `Frame` to the server, breaking it into multiple frames if
    /// necessary.
    pub fn write(&mut self, frame: Frame) -> AMQPResult<()> {
        // Take ownership of the `sender` so we can pass it to `send`. This
        // will fail if a previous `send` failed.
        let sender = self.sender.take().ok_or_else(|| {
            format_err!("tried to send, but there's no receiver")
        })?;

        // Send our message, and wait for the result.
        match sender.send(frame).wait() {
            // Our message was sent, and we have a new `sender`, so store it.
            Ok(sender) => {
                self.sender = Some(sender);
                Ok(())
            }
            // Our message failed to send, which means the other end of the
            // channel was dropped.
            Err(_err) => {
                Err(format_err!("receiver dropped the other end of channel"))
            }
        }
    }
}

impl Clone for WriteConnection {
    fn clone(&self) -> Self {
        Self {
            frame_max_limit: self.frame_max_limit,
            sender: self.sender.clone(),
        }
    }
}
