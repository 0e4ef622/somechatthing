trait __InnerDerefOk {
    type Target: ?Sized;
    type E;
    fn deref_ok(&self) -> Result<&Self::Target, &Self::E>;
}
impl<T: std::ops::Deref, E> __InnerDerefOk for Result<T, E> {
    type Target = T::Target;
    type E = E;
    fn deref_ok(&self) -> Result<&T::Target, &E> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(e),
        }
    }
}
use std::{
    borrow::Cow,
    net::{TcpStream, ToSocketAddrs},
    io::{self, prelude::*},
};

fn ping(mut conn: &TcpStream) -> io::Result<()> {
    eprintln!("sending ping to server");
    conn.write_all(&[1])?;
    conn.flush()?;
    eprintln!("ping sent, waiting for pong");
    Ok(())
}

fn send(mut conn: &TcpStream, msg: &str) -> io::Result<()> {
    let len_bytes = u64::to_be_bytes(msg.len() as u64);
    eprintln!("sending message to server");
    let buf = [
        2,
        len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3],
        len_bytes[4], len_bytes[5], len_bytes[6], len_bytes[7],
    ];
    conn.write_all(&buf)?;
    conn.write_all(msg.as_bytes())?; conn.flush()?; eprintln!("message sent (ack not implemented yet)"); Ok(())
}

fn recv_loop_inner(mut conn: &TcpStream) -> io::Result<()> {
    let mut read_buf = [0];
    conn.read_exact(&mut read_buf)
        .map_err(|e| {
            use std::io::ErrorKind;
            match e.kind() {
                ErrorKind::UnexpectedEof => panic!("connection closed"),
                _ => e,
            }
        })?;
    println!();
    match read_buf[0] {
        1 => eprintln!("pong received"),
        _ => eprintln!("unknown data received"),
    }
    print!("> ");
    io::stdout().flush()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr_str: Cow<'_, str> = match std::env::var("SCT_ADDR") {
        Ok(s) => s.into(),
        Err(std::env::VarError::NotPresent) => "localhost:2000".into(),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("Invalid unicode in SCT_ADDR".into());
        }
    };
    let addr = addr_str.to_socket_addrs()?;
    let addr = addr.as_slice();
    eprintln!("connecting to {}", addr_str);
    let conn = TcpStream::connect(addr)?;
    eprintln!("connected");
    if let Ok(_) = std::env::var("SCT_NOREADLINE") { // don't use rustyline
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        let stdin = std::io::stdin();
        let stdin = stdin.lock();
        let mut bufstdin = std::io::BufReader::new(stdin);
        let id: u64 = {
            'l: loop {
                write!(stdout, "Enter an id: ")?;
                stdout.flush()?;
                let line = bufstdin.by_ref().lines().next();
                match line.map(|l| l.map(|s| s.parse::<u64>())) {
                    Some(Ok(Ok(id))) => break 'l id,
                    Some(Ok(Err(e))) => eprintln!("Invalid id: {}", e),
                    Some(Err(e)) => {
                        eprintln!("{}", e);
                        Err(e)?
                    }
                    None => return Ok(()),
                }
            }
        };
        let id_bytes = id.to_be_bytes();
        (&conn).write(&id_bytes);
        crossbeam::scope(|s| {
            s.spawn(|_| loop { recv_loop_inner(&conn); });
            loop {
                write!(stdout, "> ")?;
                stdout.flush()?;
                let line = bufstdin.by_ref().lines().next();
                match line.as_ref().map(|r| r.deref_ok().map(|s| {let mut x = s.splitn(2, ' '); (x.next().unwrap(), x.next())})) {
                    Some(Ok(("ping", None))) => ping(&conn)?,
                    Some(Ok(("ping", Some(_)))) => eprintln!("ping doesn't take any arguments"),
                    Some(Ok(("send", rest))) => send(&conn, rest.unwrap_or(""))?,
                    Some(Ok((c, _))) => eprintln!("Unrecognized command `{}'", c),
                    Some(Err(e)) => {
                        eprintln!("{}", e);
                        break;
                    }
                    None => break,
                }
            }
            Ok(())
        }).unwrap()
    } else {
        let mut rl = rustyline::Editor::<()>::new();
        let id = loop {
            match rl.readline("Enter an id: ").map(|s| s.parse::<u64>()) {
                Ok(Ok(id)) => break id,
                Ok(Err(e)) => println!("Invalid id: {}", e),
                Err(_) => return Ok(()),
            }
        };
        let id_bytes = id.to_be_bytes();
        (&conn).write(&id_bytes);
        crossbeam::scope(|s| {
            s.spawn(|_| loop { recv_loop_inner(&conn); });
            loop {
                let line = rl.readline("> ");
                match line.deref_ok().map(|s| {let mut x = s.splitn(2, ' '); (x.next().unwrap(), x.next())}) {
                    Ok(("ping", None)) => ping(&conn)?,
                    Ok(("ping", Some(_))) => eprintln!("ping doesn't take any arguments"),
                    Ok(("send", rest)) => send(&conn, rest.unwrap_or(""))?,
                    Ok(_) => eprintln!("Unrecognized command"),

                    Err(e) => {
                        eprintln!("{}", e);
                        break;
                    }
                }
            }
            Ok(())
        }).unwrap()
    }
}
