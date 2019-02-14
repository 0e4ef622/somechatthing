use std::error::Error;

use bytes::BytesMut;
use tokio::{
    prelude::*,
    codec::{Decoder, Encoder},
};

pub enum ClientMessage {
    Ping,
    Send(Vec<u8>),
}

#[derive(Debug)]
pub enum DecodeError {
    MalformedInput,
    Tokio(tokio::io::Error),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DecodeError::MalformedInput => write!(f, "malformed input"),
            DecodeError::Tokio(e) => write!(f, "io error: {}", e),
        }
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DecodeError::MalformedInput => None,
            DecodeError::Tokio(e) => Some(e),
        }
    }
}

impl From<tokio::io::Error> for DecodeError {
    fn from(e: tokio::io::Error) -> Self {
        DecodeError::Tokio(e)
    }
}

pub struct MsgDecoder;

impl Decoder for MsgDecoder {
    type Item = ClientMessage;
    type Error = DecodeError;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<ClientMessage>, DecodeError> {
        match src.get(0) {
            Some(1) => {
                src.advance(1);
                Ok(Some(ClientMessage::Ping))
            }
            Some(2) => {
                let length = match src.get(1..9) {
                    Some(n) => u64::from_be_bytes([n[0], n[1], n[2], n[3], n[4], n[5], n[6], n[7]]) as usize,
                    None => return Ok(None),
                };
                let message = match src.get(9..9+length) {
                    Some(s) => s.to_owned(),
                    None => return Ok(None),
                };
                src.advance(9 + length);
                Ok(Some(ClientMessage::Send(message)))
            }
            Some(_) => Err(DecodeError::MalformedInput),
            None => Ok(None),
        }
    }
}
