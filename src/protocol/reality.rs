use rustls::Connection;

pub struct RealityStream<S> {
    inner: S,
    conn: Connection
}