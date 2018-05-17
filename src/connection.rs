use failure;
use futures::{Future, Sink, Stream, sync::mpsc};

/// For this toy implementation, our `Frame`s are just single lines of text.
type Frame = String;

/// For this toy implementation, just use `failure::Error`.
type AMQPError = failure::Error;

/// Our result type.
type AMQPResult<T> = Result<T, failure::Error>;

/// A connection to an AMQP server.
pub struct Connection {

}

impl Connection {
    /// Open a TLS connection to the specified address.
    #[cfg(feature = "tls")]
    pub fn open_tls(host: &str, port: u16) -> AMQPResult<Connection> {
        unimplemented!()
    }

    /// Open a regular TCP connection to the specified address.
    pub fn open(host: &str, port: u16) -> AMQPResult<Connection> {
        unimplemented!()
    }

    /// Split this connection into an independent `(ReadConnection,
    /// WriteConnection)` pair.
    pub fn split(self) -> (ReadConnection, WriteConnection) {
        unimplemented!()
    }
}

/// A connection which can read frames from an AMQP server.
pub struct ReadConnection {
    receiver: mpsc::Receiver<Frame>,
}

impl ReadConnection {
    /// Read the next frame. Blocking.
    pub fn read(&mut self) -> AMQPResult<Frame> {
        unimplemented!()
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
