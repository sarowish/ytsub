use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use itertools::Itertools;
use serde_json::Value;

#[derive(Debug, PartialEq)]
enum WireType {
    /// int32, int64, uint32, uint64, sint32, sint64, bool, enum
    Varint,
    /// fixed64, sfixed64, double
    I64,
    /// string, bytes, embedded messages, packed repeated fields
    Len,
    /// group start (deprecated)
    SGroup,
    /// group end (deprecated)
    EGroup,
    /// fixed32, sfixed32, float
    I32,
}

impl TryFrom<u64> for WireType {
    type Error = anyhow::Error;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let id = match value {
            0 => WireType::Varint,
            1 => WireType::I64,
            2 => WireType::Len,
            3 => WireType::SGroup,
            4 => WireType::EGroup,
            5 => WireType::I32,
            _ => return Err(anyhow::anyhow!("invalid wire type id")),
        };

        Ok(id)
    }
}

#[derive(Debug, PartialEq)]
struct Tag {
    field_number: u64,
    wire_type: WireType,
}

fn parse_varint<T: Iterator<Item = u8>>(message: &mut T) -> Option<u64> {
    const MSB_MASK: u8 = 1 << 7;

    message
        .take_while_inclusive(|byte| byte & MSB_MASK != 0)
        .enumerate()
        .map(|(idx, byte)| u64::from(byte & !MSB_MASK) << (7 * idx))
        .reduce(|acc, byte| acc | byte)
}

fn parse_tag<T: Iterator<Item = u8>>(message: &mut T) -> Option<Tag> {
    let varint = parse_varint(message)?;

    Some(Tag {
        field_number: varint >> 3,
        wire_type: WireType::try_from(varint & 0b111).ok()?,
    })
}

fn parse_message<T: Iterator<Item = u8>>(message: &mut T) -> Option<Value> {
    let mut res = Value::default();

    while let Some(tag) = parse_tag(message) {
        let index = tag.field_number.to_string();

        match tag.wire_type {
            WireType::Varint => res[index] = parse_varint(message)?.into(),
            WireType::I64 => {
                let bytes = message.take(8).collect::<Vec<u8>>();
                res[index] = f64::from_le_bytes(bytes.try_into().ok()?).into();
            }
            WireType::Len => {
                let len = parse_varint(message)?;
                let submessage = message.take(len as usize).collect::<Vec<u8>>();

                res[index] = if let Some(s) = String::from_utf8(submessage.clone())
                    .ok()
                    .filter(|s| !s.chars().any(char::is_control))
                {
                    s.into()
                } else {
                    let submessage = parse_message(&mut submessage.into_iter()).unwrap_or_default();
                    let mut value = res.get_mut(&index);

                    if let Some(value) = &mut value
                        && value.is_object()
                    {
                        Value::Array(vec![value.take(), submessage])
                    } else if let Some(value) = value.and_then(Value::as_array_mut) {
                        value.push(submessage);
                        continue;
                    } else {
                        submessage
                    }
                }
            }
            WireType::I32 => {
                let bytes = message.take(4).collect::<Vec<u8>>();
                res[index] = f32::from_le_bytes(bytes.try_into().ok()?).into();
            }
            _ => return None,
        };
    }

    Some(res)
}

pub fn decode_protobuf_from_binary(message: Vec<u8>) -> Option<Value> {
    parse_message(&mut message.into_iter())
}

pub fn decode_protobuf(message: &str) -> Option<Value> {
    let bytes = BASE64_URL_SAFE_NO_PAD.decode(message).ok()?;

    decode_protobuf_from_binary(bytes)
}

#[cfg(test)]
mod tests {
    use crate::protobuf::{Tag, WireType, decode_protobuf, decode_protobuf_from_binary, parse_tag};

    #[test]
    fn tag() {
        let bytes: [u8; 2] = [0xf2, 0x6];

        assert_eq!(
            parse_tag(bytes.into_iter().by_ref()),
            Some(Tag {
                field_number: 110,
                wire_type: WireType::Len
            })
        );

        let bytes = [0x8_u8];

        assert_eq!(
            parse_tag(bytes.into_iter().by_ref()),
            Some(Tag {
                field_number: 1,
                wire_type: WireType::Varint
            })
        );
    }

    #[test]
    fn a_simple_message() {
        let bytes = vec![0x08, 0x96, 0x01];

        assert_eq!(
            decode_protobuf_from_binary(bytes).unwrap()["1"].as_u64(),
            Some(150)
        );
    }

    #[test]
    fn string() {
        let bytes = vec![0x12, 0x07, 0x74, 0x65, 0x73, 0x74, 0x69, 0x6e, 0x67];

        assert_eq!(
            decode_protobuf_from_binary(bytes).unwrap()["2"].as_str(),
            Some("testing")
        );
    }

    #[test]
    fn non_varint32() {
        let bytes = vec![0x5d, 0x00, 0x00, 0xde, 0x42];

        assert_eq!(
            decode_protobuf_from_binary(bytes).unwrap()["11"].as_f64(),
            Some(111.0)
        );
    }

    #[test]
    fn non_varint64() {
        let bytes = vec![0x61, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x5c, 0x40];

        assert_eq!(
            decode_protobuf_from_binary(bytes).unwrap()["12"].as_f64(),
            Some(112.0)
        );
    }

    #[test]
    fn submessage() {
        let bytes = vec![0x1a, 0x03, 0x08, 0x96, 0x01];

        assert_eq!(
            decode_protobuf_from_binary(bytes),
            Some(serde_json::json!({
                "3": { "1": 150 }
            }))
        );
    }

    #[test]
    fn yt_tab() {
        let message = "EgZ2aWRlb3PyBgQKAjoA";

        assert_eq!(
            decode_protobuf(message),
            Some(serde_json::json!({
                "2": "videos",
                "110": { "1": { "7": "" } }
            }))
        );
    }

    #[test]
    fn yt_xtags() {
        let message = "ChQKBWFjb250EgtkdWJiZWQtYXV0bwoNCgRsYW5nEgVlbi1VUw";

        assert_eq!(
            decode_protobuf(message),
            Some(serde_json::json!({
                "1": [
                    {"1": "acont", "2": "dubbed-auto"},
                    {"1": "lang", "2": "en-US"},
                ]
            }))
        );
    }
}
