use errors::*;
use std::io::{self, Read};
use rmpv::{decode, encode, Integer, Utf8String, Value};
use std::convert::From;

/// Represents a `MessagePack-RPC` message as described in the
/// [specifications](https://github.com/msgpack-rpc/msgpack-rpc/blob/master/spec.md#messagepack-rpc-protocol-specification)
#[derive(PartialEq, Clone, Debug)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

/// Represents a `MessagePack-RPC` request as described in the
/// [specifications](https://github.com/msgpack-rpc/msgpack-rpc/blob/master/spec.md#messagepack-rpc-protocol-specification)
#[derive(PartialEq, Clone, Debug)]
pub struct Request {
    pub id: u32,
    pub method: String,
    pub params: Vec<Value>,
}

/// Represents a `MessagePack-RPC` response as described in the
/// [specifications](https://github.com/msgpack-rpc/msgpack-rpc/blob/master/spec.md#messagepack-rpc-protocol-specification)
#[derive(PartialEq, Clone, Debug)]
pub struct Response {
    pub id: u32,
    pub result: Result<Value, Value>,
}

/// Represents a `MessagePack-RPC` notification as described in the
/// [specifications](https://github.com/msgpack-rpc/msgpack-rpc/blob/master/spec.md#messagepack-rpc-protocol-specification)
#[derive(PartialEq, Clone, Debug)]
pub struct Notification {
    pub method: String,
    pub params: Vec<Value>,
}

const REQUEST_MESSAGE: u64 = 0;
const RESPONSE_MESSAGE: u64 = 1;
const NOTIFICATION_MESSAGE: u64 = 2;

impl Message {
    pub fn decode<R>(rd: &mut R) -> Result<Message, DecodeError>
    where
        R: Read,
    {
        let msg = decode::value::read_value(rd)?;
        if let Value::Array(ref array) = msg {
            if array.len() < 3 {
                // notification are the shortest message and have 3 items
                return Err(DecodeError::Invalid);
            }
            if let Value::Integer(msg_type) = array[0] {
                match msg_type.as_u64() {
                    Some(REQUEST_MESSAGE) => {
                        return Ok(Message::Request(Request::decode(array)?));
                    }
                    Some(RESPONSE_MESSAGE) => {
                        return Ok(Message::Response(Response::decode(array)?));
                    }
                    Some(NOTIFICATION_MESSAGE) => {
                        return Ok(Message::Notification(Notification::decode(array)?));
                    }
                    _ => {
                        return Err(DecodeError::Invalid);
                    }
                }
            } else {
                return Err(DecodeError::Invalid);
            }
        } else {
            return Err(DecodeError::Invalid);
        }
    }

    pub fn as_value(&self) -> Value {
        match *self {
            Message::Request(Request {
                id,
                ref method,
                ref params,
            }) => Value::Array(vec![
                Value::Integer(Integer::from(REQUEST_MESSAGE)),
                Value::Integer(Integer::from(id)),
                Value::String(Utf8String::from(method.as_str())),
                Value::Array(params.clone()),
            ]),
            Message::Response(Response { id, ref result }) => {
                let (error, result) = match *result {
                    Ok(ref result) => (Value::Nil, result.to_owned()),
                    Err(ref err) => (err.to_owned(), Value::Nil),
                };
                Value::Array(vec![
                    Value::Integer(Integer::from(RESPONSE_MESSAGE)),
                    Value::Integer(Integer::from(id)),
                    error,
                    result,
                ])
            }
            Message::Notification(Notification {
                ref method,
                ref params,
            }) => Value::Array(vec![
                Value::Integer(Integer::from(NOTIFICATION_MESSAGE)),
                Value::String(Utf8String::from(method.as_str())),
                Value::Array(params.to_owned()),
            ]),
        }
    }

    pub fn pack(&self) -> io::Result<Vec<u8>> {
        let mut bytes = vec![];
        encode::write_value(&mut bytes, &self.as_value())?;
        Ok(bytes)
    }
}

impl Notification {
    fn decode(array: &[Value]) -> Result<Self, DecodeError> {
        if array.len() < 3 {
            return Err(DecodeError::Invalid);
        }

        let method = if let Value::String(ref method) = array[1] {
            method
                .as_str()
                .and_then(|s| Some(s.to_string()))
                .ok_or(DecodeError::Invalid)?
        } else {
            return Err(DecodeError::Invalid);
        };

        let params = if let Value::Array(ref params) = array[2] {
            params.clone()
        } else {
            return Err(DecodeError::Invalid);
        };

        Ok(Notification {
            method: method,
            params: params,
        })
    }
}

impl Request {
    fn decode(array: &[Value]) -> Result<Self, DecodeError> {
        if array.len() < 4 {
            return Err(DecodeError::Invalid);
        }

        let id = if let Value::Integer(id) = array[1] {
            id.as_u64()
                .and_then(|id| Some(id as u32))
                .ok_or(DecodeError::Invalid)?
        } else {
            return Err(DecodeError::Invalid);
        };

        let method = if let Value::String(ref method) = array[2] {
            method
                .as_str()
                .and_then(|s| Some(s.to_string()))
                .ok_or(DecodeError::Invalid)?
        } else {
            return Err(DecodeError::Invalid);
        };

        let params = if let Value::Array(ref params) = array[3] {
            params.clone()
        } else {
            return Err(DecodeError::Invalid);
        };

        Ok(Request {
            id: id,
            method: method,
            params: params,
        })
    }
}

impl Response {
    fn decode(array: &[Value]) -> Result<Self, DecodeError> {
        if array.len() < 2 {
            return Err(DecodeError::Invalid);
        }

        let id = if let Value::Integer(id) = array[1] {
            id.as_u64()
                .and_then(|id| Some(id as u32))
                .ok_or(DecodeError::Invalid)?
        } else {
            return Err(DecodeError::Invalid);
        };

        match array[2] {
            Value::Nil => Ok(Response {
                id: id,
                result: Ok(array[3].clone()),
            }),
            ref error => Ok(Response {
                id: id,
                result: Err(error.clone()),
            }),
        }
    }
}

#[test]
fn test_decode_request() {
    let valid = Message::Request(Request {
        id: 1234,
        method: "dummy".to_string(),
        params: Vec::new(),
    });
    let bytes = valid.pack().unwrap();

    // valid message
    {
        let mut buf = io::Cursor::new(&bytes);
        assert_eq!(valid, Message::decode(&mut buf).unwrap());
    }

    // truncated
    {
        let bytes = Vec::from(&bytes[0..bytes.len() - 1]);
        let mut buf = io::Cursor::new(&bytes);
        assert!(match Message::decode(&mut buf) {
            Err(DecodeError::Truncated) => true,
            _ => false,
        });
    }

    // invalid message type
    {
        let mut bytes = Vec::from(&bytes[..]);
        bytes[1] = 5;
        let mut buf = io::Cursor::new(&bytes);
        assert!(match Message::decode(&mut buf) {
            Err(DecodeError::Invalid) => true,
            _ => false,
        });
    }
}
