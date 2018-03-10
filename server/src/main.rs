extern crate cyclotron_backend;
extern crate docopt;
extern crate hyper;
#[macro_use]
extern crate serde_derive;
extern crate websocket;
extern crate futures;
extern crate failure;
extern crate serde_json;

use std::fs::{
    File,
};
use std::net::{
    Ipv4Addr,
    SocketAddr,
    SocketAddrV4,
    TcpStream,
};
use std::io::{
    self,
    BufRead,
    BufReader,
    Read,
};
use std::time::Duration;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use failure::Error;
use futures::{
    future,
};
use futures::future::Future;
use docopt::Docopt;
use hyper::{
    Method,
    StatusCode,
};
use hyper::server::{
    Http,
    NewService,
    Request,
    Response,
    Service,
};
use websocket::Message;
use websocket::server::upgrade::WsUpgrade;
use websocket::server::upgrade::sync::Buffer;
use websocket::sync::Server;

struct Inner {
    trace_path: PathBuf,
    frontend_dir: PathBuf,
}

#[derive(Clone)]
struct CyclotronServer {
    inner: Arc<Mutex<Inner>>,
}

impl CyclotronServer {
    fn new(args: &Args) -> Self {
        let inner = Inner {
            trace_path: PathBuf::from(&args.flag_trace),
            frontend_dir: PathBuf::from("../frontend"),
        };
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    fn serve_frontend(&self, p: &str) -> Response {
        let inner = self.inner.lock().unwrap();
        let path = inner.frontend_dir.clone().join(p.trim_left_matches("/"));

        let mut response = Response::new();
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                println!("Failed to open {:?}: {:?}", path, e);
                response.set_status(StatusCode::NotFound);
                return response;
            },
        };
        match Read::bytes(file).collect::<Result<Vec<u8>, _>>() {
            Ok(bytes) => response.set_body(bytes),
            Err(e) => {
                println!("Failed to read file {:?}", e);
                response.set_status(StatusCode::InternalServerError);
                return response;
            },
        }
        response
    }

    fn stream(&self, conn: WsUpgrade<TcpStream, Option<Buffer>>) -> Result<(), Error> {
        if !conn.protocols().contains(&"cyclotron-ws".into()) {
            conn.reject().map_err(|(_, e)| e)?;
            return Ok(());
        }
        let mut client = conn.use_protocol("cyclotron-ws")
            .accept()
            .map_err(|(_, e)| e)?;
        println!("New connection from {:?}", client.peer_addr()?);

        let mut file = {
            let inner = self.inner.lock().unwrap();
            BufReader::new(File::open(&inner.trace_path)?)
        };

        // First, push the whole file over the socket
        let mut fragment = loop {
            let mut buf = String::new();
            let num_read = file.read_line(&mut buf)?;

            if num_read == 0 || !buf.ends_with("\n") {
                break buf;
            } else {
                buf.pop();
                // let event: TraceEvent = serde_json::from_str(&buf)?;
                // println!("Read {:?}", event);
                client.send_message(&Message::text(buf.as_str()))?;
            }
        };

        loop {
            let num_read = file.read_line(&mut fragment)?;

            if num_read == 0 || !fragment.ends_with("\n") {
                // Just poll, sigh.
                thread::sleep(Duration::from_millis(250));
                continue;
            }

            fragment.pop();
            // let event: TraceEvent = serde_json::from_str(&fragment)?;
            // println!("Read {:?}", event);
            client.send_message(&Message::text(fragment.as_str()))?;

            fragment.clear();
        }

    }
}

impl NewService for CyclotronServer {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Instance = Self;

    fn new_service(&self) -> Result<Self, io::Error> {
        Ok(self.clone())
    }
}

impl Service for CyclotronServer {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        match (req.method(), req.path()) {
            (&Method::Get, "/") => {
                let response = self.serve_frontend("/src/index.html");
                Box::new(future::ok(response))
            },
            (&Method::Get, p) => {
                let response = self.serve_frontend(p);
                Box::new(future::ok(response))
            },
            _ => {
                let mut response = Response::new();
                response.set_status(StatusCode::NotFound);
                Box::new(future::ok(response))
            },
        }
    }
}

const USAGE: &'static str = "
Cyclotron trace server.

Usage:
   cyclotron-server --http=<port> --ws=<port> --trace=<path>
   cyclotron-server (-h | --help)

Options:
  -h --help       Show this screen.
  --http=<port>   Port for HTTP server
  --ws=<port>     Port for websocket server
  --trace=<path>  Path to trace file to stream in
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_http: u16,
    flag_ws: u16,
    flag_trace: String,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());
    let server = CyclotronServer::new(&args);
    let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), args.flag_http));

    let cyclotron = server.clone();
    thread::spawn(move || {
        let cyclotron = cyclotron;
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), args.flag_ws));
        let ws_server = Server::bind(addr).unwrap();

        for connection in ws_server.filter_map(Result::ok) {
            // Spawn a thread per connection
            let cyclotron_ = cyclotron.clone();
            thread::spawn(move || match cyclotron_.stream(connection) {
                Ok(_) => (),
                Err(e) => println!("Failed on stream: {:?}", e),
            });
        }
    });

    Http::new().bind(&addr, server).unwrap().run().unwrap();
}
