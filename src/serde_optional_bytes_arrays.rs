use serde::de::{Error, Visitor};
use serde::ser::SerializeSeq;
use serde::{Serializer, Deserializer};

use serde_bytes::{Bytes, ByteBuf};

pub(crate) fn serialize<const N: usize, S>(
    bytes_arrays: &Vec<Option<[u8; N]>>, serializer: S
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq_arrays = 
        serializer.serialize_seq(Some(bytes_arrays.len()))?;
    for byte_array in bytes_arrays.iter() {
        let wrapped = match byte_array {
            Some(byte_array) => Some(Bytes::new(byte_array)),
            None => None,
        };
        seq_arrays.serialize_element(&wrapped)?;
    }
    seq_arrays.end()
}

struct VecVisitor;


impl<'de> Visitor<'de> for VecVisitor {
    type Value = Vec<Option<ByteBuf>>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) 
    -> std::fmt::Result 
    {
        formatter.write_str("data is not optional byte arrays")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>, 
    {
        let mut arrays = Vec::new();
        loop {
            let next: Result<Option<Option<ByteBuf>>, _> = seq.next_element();
            match next {
                Ok(array) => 
                    if let Some(array) = array {
                        arrays.push(array)
                    } else {
                        break
                    },
                Err(e) => return Err(e),
            }
        }
        Ok(arrays)
    }

}

pub(crate) fn deserialize<'de, D, const N: usize>(deserializer: D) 
-> Result<Vec<Option<[u8; N]>>, D::Error>
where
    D: Deserializer<'de>,
{
    let arrays_generic 
        = deserializer.deserialize_seq(VecVisitor)?;
    let mut arrays_typed = Vec::new();
    for array in arrays_generic {
        if let Some(array) = array {
            let array_typed = ((&array) as &[u8]).try_into().map_err(
            |_| {
                let expected = format!("[u8; {}]", N);
                D::Error::invalid_length(array.len(), &expected.as_str())
            })?;
            arrays_typed.push(Some(array_typed))
        } else {
            arrays_typed.push(None)
        }
    }
    Ok(arrays_typed)
}