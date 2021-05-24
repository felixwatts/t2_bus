use crate::err::{BusError, BusResult};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{fmt, io::prelude::*};

pub trait PublishProtocol: Serialize + DeserializeOwned + Send + 'static + std::fmt::Debug {
    fn prefix() -> &'static str;
}

pub trait RequestProtocol: Serialize + DeserializeOwned + Send + 'static {
    fn prefix() -> &'static str;

    fn json_to_cbor(json: serde_json::Value) -> BusResult<Vec<u8>> {
        let obj: Self = serde_json::from_value(json)?;
        let cbor = serde_cbor::to_vec(&obj)?;
        Ok(cbor)
    }

    fn cbor_to_json(cbor: &Vec<u8>) -> BusResult<serde_json::Value> {
        let obj: Self::Rsp = serde_cbor::from_slice(cbor)?;
        let json = serde_json::to_value(&obj)?;
        Ok(json)
    }

    type Rsp: Serialize + DeserializeOwned + Send + 'static;
}

pub type MsgId = u32;

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct MsgSub {
    pub topic: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct MsgUnsub {
    pub topic: String,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct MsgPub {
    pub topic: String,
    pub payload: Payload,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct MsgSrv {
    pub topic: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct MsgUnsrv {
    pub topic: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct MsgReq {
    pub topic: String,
    pub payload: Payload,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub enum MsgRspStatus {
    Ok,
    Timeout,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct MsgRsp {
    pub req_id: MsgId,
    pub status: MsgRspStatus,
    pub payload: Payload,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct MsgAck {
    pub msg_id: MsgId,
    pub err: Option<BusError>,
    pub num_recipients: Option<usize>, // TODO usize in msg
}

// The message type sent from the client to the server
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub enum ProtocolClient {
    Sub(MsgSub),
    Pub(MsgPub),
    Srv(MsgSrv),
    Req(MsgReq),
    Rsp(MsgRsp),
    Unsub(MsgUnsub),
    Unsrv(MsgUnsrv),
    Stop,
}

// The message type sent from the server to the client
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub enum ProtocolServer {
    Pub(MsgPub),
    Req(MsgReq),
    Rsp(MsgRsp),
    Ack(MsgAck),
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct Msg<T> {
    pub id: MsgId,
    pub content: T,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct Payload {
    data: Vec<u8>,
    is_compressed: bool,
}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} bytes", self.data.len())?;
        if self.is_compressed {
            write!(f, " compressed")?;
        }
        Ok(())
    }
}

const PAYLOAD_COMPRESSION_THRESHOLD_BYTES: usize = 5000;

impl From<Vec<u8>> for Payload {
    fn from(data: Vec<u8>) -> Self {
        if data.len() > PAYLOAD_COMPRESSION_THRESHOLD_BYTES {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data[..]).unwrap();
            Payload {
                data: encoder.finish().unwrap(),
                is_compressed: true,
            }
        } else {
            Payload {
                data,
                is_compressed: false,
            }
        }
    }
}

impl From<Payload> for Vec<u8> {
    fn from(payload: Payload) -> Self {
        match payload.is_compressed {
            true => {
                let mut decoder = GzDecoder::new(&payload.data[..]);
                let mut data = Vec::new();
                decoder.read_to_end(&mut data).unwrap();
                data
            }
            false => payload.data,
        }
    }
}
