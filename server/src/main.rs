use actix::prelude::*;
use tokio::net::TcpListener;
use futures::prelude::*;

pub mod codec;

pub mod conn_mgr {
    use actix::prelude::*;
    use tokio::net::TcpStream;

    #[derive(Default)]
    pub struct ConnectionsManager {
        connections: Vec<Addr<crate::client_sess::Session>>,
    }

    impl Actor for ConnectionsManager {
        type Context = Context<Self>;
    }

    impl Supervised for ConnectionsManager {}
    impl SystemService for ConnectionsManager {}

    pub struct Connect(pub TcpStream);

    impl Message for Connect {
        type Result = ();
    }

    impl Handler<Connect> for ConnectionsManager {
        type Result = ();
        fn handle(&mut self, msg: Connect, _ctx: &mut Self::Context) {
            let addr = crate::client_sess::Session::new(msg.0).start();
            self.connections.push(addr);
        }
    }

    pub struct Stopped(pub Addr<crate::client_sess::Session>);

    impl Message for Stopped {
        type Result = ();
    }

    impl Handler<Stopped> for ConnectionsManager {
        type Result = ();
        fn handle(&mut self, msg: Stopped, _ctx: &mut Self::Context) {
            self.connections.retain(|a| a != &msg.0);
        }
    }
}

pub mod client_sess {
    use crate::codec::{ClientMessage, DecodeError, MsgDecoder};

    use actix::prelude::*;
    use tokio::{
        prelude::*,
        codec,
        io::WriteHalf,
        net::TcpStream,
    };

    pub struct Session {
        /// Only exists when being constructed, `Actor::started()` will consume this to set up the
        /// message stream.
        connection: Option<TcpStream>,
        /// Only exists after the actor receives a `Ready` message from itself, when the connection
        /// is split and the read half becomes part of the message stream.
        write_half: Option<WriteHalf<TcpStream>>,
        id: u64,
    }

    impl Session {
        pub fn new(conn: TcpStream) -> Self {
            Session {
                connection: Some(conn),
                write_half: None,
                id: 0,
            }
        }
    }

    impl Actor for Session {
        type Context = Context<Self>;
        fn started(&mut self, ctx: &mut Self::Context) {
            eprintln!("Incoming connection from {}", self.connection.as_ref().unwrap().peer_addr().unwrap());
            let addr = ctx.address();
            // wait for the client to send it's id
            let fut = tokio::io::read_exact(self.connection.take().unwrap(), [0; 8])
                .map_err(|e| println!("{}", e))
                .and_then(move |(c, id)| {
                    addr.send(Ready(c, u64::from_be_bytes(id)))
                        .map_err(|_| ())
                });
            actix::spawn(fut);
        }
        fn stopped(&mut self, ctx: &mut Self::Context) {
            System::current()
                .registry()
                .get::<crate::conn_mgr::ConnectionsManager>()
                .do_send(crate::conn_mgr::Stopped(ctx.address()));
        }
    }
    struct Ready(TcpStream, u64);
    impl Message for Ready {
        type Result = ();
    }
    impl Handler<Ready> for Session {
        type Result = ();
        fn handle(&mut self, Ready(conn, id): Ready, ctx: &mut Self::Context) {
            eprintln!("{} has identified itself as id {}", conn.peer_addr().unwrap(), id);
            let (read_half, write_half) = conn.split();

            ctx.add_stream(codec::FramedRead::new(read_half, MsgDecoder));

            self.write_half = Some(write_half);
            self.id = id;
        }
    }

    impl StreamHandler<ClientMessage, DecodeError> for Session {
        fn handle(&mut self, msg: ClientMessage, ctx: &mut Self::Context) {
            let id = self.id;
            match msg {
                ClientMessage::Ping => {
                    eprintln!("received ping from {}", self.id);
                    let write_half = self.write_half.take().unwrap();
                    let fut = tokio::io::write_all(write_half, [1]);
                    let fut = actix::fut::wrap_future(fut)
                        .map(|(write_half, _), actor: &mut Session, _ctx| actor.write_half = Some(write_half))
                        .map_err(move |e, _actor, ctx| {
                            eprintln!("error sending pong to {}: {}", id, e);
                            ctx.stop();
                        });
                    ctx.wait(fut);
                }
                ClientMessage::Send(msg) => {
                    eprintln!("{} says \"{}\"", self.id, String::from_utf8_lossy(&msg));
                }
            }
        }
    }
}

fn main() {
    System::run(|| {
        let conn_mgr_addr = conn_mgr::ConnectionsManager::create(|ctx| {
            ctx.add_message_stream(
                TcpListener::bind(&"0.0.0.0:2000".parse().unwrap())
                    .unwrap()
                    .incoming()
                    .map(conn_mgr::Connect)
                    .map_err(|_| ())
            );
            conn_mgr::ConnectionsManager::default()
        });
        System::current().registry().set(conn_mgr_addr);
    });
}
