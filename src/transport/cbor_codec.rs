use crate::err::*;
use bytes::Buf;
use bytes::BufMut;
use serde::{de::DeserializeOwned, Serialize};
use std::io::Cursor;
use std::marker::PhantomData;

/// The internal codec used within the bus. Can be useful in conjunction with `Client::publish_bytes`, `Client::request_bytes`, `Client::request_bytes_into` and `Client::subscribe_bytes_into`.
#[derive(Default)]
pub struct CborCodec<TEncode, TDecode> {
    msg_len: Option<usize>,
    e: PhantomData<TEncode>,
    d: PhantomData<TDecode>,
}

impl<TEncode, TDecode> CborCodec<TEncode, TDecode>
where
    TEncode: Serialize,
{
    pub fn new() -> CborCodec<TEncode, TDecode> {
        CborCodec {
            e: PhantomData {},
            d: PhantomData {},
            msg_len: None,
        }
    }
}

impl<TEncode, TDecode> tokio_util::codec::Decoder for CborCodec<TEncode, TDecode>
where
    TEncode: Serialize,
    TDecode: DeserializeOwned,
{
    type Item = TDecode;
    type Error = BusError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let buf_len = src.len();

        match self.msg_len {
            None => {
                if buf_len < 4 {
                    return Ok(None);
                } else {
                    let msg_len = src.get_u32() as usize;
                    src.reserve(msg_len);
                    self.msg_len = Some(msg_len);
                }
            }
            Some(_) => {}
        }

        let buf_len = src.len();

        match self.msg_len {
            Some(msg_len) => {
                if buf_len < msg_len {
                    Ok(None)
                } else {
                    let msg_res = deser(&src[0..msg_len]);
                    match msg_res {
                        Ok(msg) => {
                            src.advance(msg_len);
                            self.msg_len = None;
                            Ok(Some(msg))
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            None => Ok(None),
        }
    }
}

impl<TEncode: Serialize, TDecode> tokio_util::codec::Encoder<TEncode>
    for CborCodec<TEncode, TDecode>
{
    type Error = BusError;

    fn encode(&mut self, item: TEncode, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        let bytes = ser(&item)?;
        dst.put_u32(bytes.len() as u32);
        dst.put_slice(&bytes);
        Ok(())
    }
}

/// Serialize using the bus' internal serialization format
pub fn ser<T: Serialize>(value: &T) -> BusResult<Vec<u8>> {
    let mut data = vec![];
    ciborium::into_writer(value, &mut data)?;
    Ok(data)
} 

/// Deserialize using the bus' internal serialization format
pub fn deser<T: DeserializeOwned>(data: &[u8]) -> BusResult<T> {
    let reader = Cursor::new(data);
    let t = ciborium::from_reader(reader)?;
    Ok(t)
}
