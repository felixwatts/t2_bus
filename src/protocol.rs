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
        let cbor = crate::transport::cbor_codec::ser(&obj)?;
        Ok(cbor)
    }

    fn cbor_to_json(cbor: &[u8]) -> BusResult<serde_json::Value> {
        let obj: Self::Rsp = crate::transport::cbor_codec::deser(cbor)?;
        let json = serde_json::to_value(&obj)?;
        Ok(json)
    }

    type Rsp: Serialize + DeserializeOwned + Send + 'static;
}

pub type MsgId = u32;

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct SubMsg {
    pub topic: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct UnsubMsg {
    pub topic: String,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct PubMsg {
    pub topic: String,
    pub payload: Payload,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct SrvMsg {
    pub topic: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct UnsrvMsg {
    pub topic: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct ReqMsg {
    pub topic: String,
    pub payload: Payload,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub enum RspMsgStatus {
    Ok,
    Timeout,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct RspMsg {
    pub req_id: MsgId,
    pub status: RspMsgStatus,
    pub payload: Payload,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct AckMsg {
    pub msg_id: MsgId,
    pub err: Option<BusError>,
    pub num_recipients: Option<usize>, // TODO usize in msg
}

// The message type sent from the client to the server
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub enum ProtocolClient {
    Sub(SubMsg),
    Pub(PubMsg),
    Srv(SrvMsg),
    Req(ReqMsg),
    Rsp(RspMsg),
    Unsub(UnsubMsg),
    Unsrv(UnsrvMsg),
    Stop,
}

// The message type sent from the server to the client
#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub enum ProtocolServer {
    Pub(PubMsg),
    Req(ReqMsg),
    Rsp(RspMsg),
    Ack(AckMsg),
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
