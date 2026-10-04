//! Bounded, synchronous framing for a future local child (no transport or model lifecycle here).
//!
//! Wire format: 4-byte unsigned big-endian payload length, then exactly that many
//! UTF-8 JSON bytes (1..=64 KiB). One call decodes one frame; it never scans for
//! another frame after an invalid one. A read or write failure is terminal for the
//! channel: the caller must discard it, never retry on the same stream. Framing
//! does not authenticate a response: a future supervisor must validate its run ID.

use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Read one complete, typed JSON frame without consuming any bytes of the next.
/// The caller supplies a strict schema (e.g. `#[serde(deny_unknown_fields)]`).
pub(crate) fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> io::Result<T> {
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;
    let len = u32::from_be_bytes(header) as usize;
    if !(1..=MAX_FRAME_BYTES).contains(&len) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame length",
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Serialize before writing the header, so an oversized or invalid value writes nothing.
/// I/O failure may leave a partial frame; discard the channel after any error.
pub(crate) fn write_frame<W: Write, T: Serialize>(writer: &mut W, value: &T) -> io::Result<()> {
    let mut payload = [0u8; MAX_FRAME_BYTES];
    let mut remaining = &mut payload[..];
    serde_json::to_writer(&mut remaining, value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let len = MAX_FRAME_BYTES - remaining.len();
    if len == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty frame"));
    }
    writer.write_all(&(len as u32).to_be_bytes())?;
    writer.write_all(&payload[..len])
}

#[cfg(test)]
mod tests {
    use super::{MAX_FRAME_BYTES, read_frame, write_frame};
    use serde::{Deserialize, Serialize};
    use std::io::{self, Cursor, Write};

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reply {
        run_id: String,
        status: String,
    }

    #[test]
    fn round_trip_one_typed_frame_without_consuming_next() {
        let first = Reply {
            run_id: "run-1".into(),
            status: "pending".into(),
        };
        let second = Reply {
            run_id: "run-2".into(),
            status: "done".into(),
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &first).unwrap();
        let first_len = bytes.len();
        write_frame(&mut bytes, &second).unwrap();
        let mut input = Cursor::new(bytes);
        assert_eq!(read_frame::<_, Reply>(&mut input).unwrap(), first);
        assert_eq!(input.position() as usize, first_len);
        assert_eq!(read_frame::<_, Reply>(&mut input).unwrap(), second);
    }

    fn framed(body: &[u8]) -> Vec<u8> {
        let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(body);
        bytes
    }

    fn rejected(bytes: &[u8], kind: io::ErrorKind) {
        assert_eq!(
            read_frame::<_, Reply>(&mut Cursor::new(bytes))
                .unwrap_err()
                .kind(),
            kind
        );
    }

    #[test]
    fn rejects_short_header_zero_and_oversize_before_body_read() {
        rejected(&[0, 0, 0], io::ErrorKind::UnexpectedEof);
        rejected(&[0, 0, 0, 0], io::ErrorKind::InvalidData);
        let mut input = Cursor::new(((MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
        assert_eq!(
            read_frame::<_, Reply>(&mut input).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(input.position(), 4);
        rejected(&u32::MAX.to_be_bytes(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_truncation_and_malformed_payloads_without_scanning_forward() {
        let mut truncated = framed(br#"{"run_id":"r","status":"ok"}"#);
        truncated.pop();
        rejected(&truncated, io::ErrorKind::UnexpectedEof);
        for body in [
            &b"x"[..],
            &b"\xff"[..],
            &br#"{"run_id":"r","status":}"#[..],
            &br#"{"run_id":"r","status":"ok"}x"#[..],
            &br#"{"run_id":"r","status":"ok","path":"/secret"}"#[..],
            &br#"{"run_id":"r"}"#[..],
            &br#"{"run_id":42,"status":"ok"}"#[..],
        ] {
            rejected(&framed(body), io::ErrorKind::InvalidData);
        }
        let mut bytes = framed(b"x");
        bytes.extend(framed(br#"{"run_id":"later","status":"ok"}"#));
        let mut input = Cursor::new(bytes);
        assert_eq!(
            read_frame::<_, Reply>(&mut input).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(input.position(), 5); // Do not accept the later frame as this reply.
    }

    #[test]
    fn exact_payload_boundary_is_accepted_and_one_byte_more_is_not_written() {
        let mut reply = Reply {
            run_id: String::new(),
            status: "ok".into(),
        };
        let overhead = serde_json::to_vec(&reply).unwrap().len();
        reply.run_id = "a".repeat(MAX_FRAME_BYTES - overhead);
        let mut output = Vec::new();
        write_frame(&mut output, &reply).unwrap();
        assert_eq!(&output[..4], &(MAX_FRAME_BYTES as u32).to_be_bytes());
        assert_eq!(output.len(), MAX_FRAME_BYTES + 4);
        assert_eq!(
            read_frame::<_, Reply>(&mut Cursor::new(&output)).unwrap(),
            reply
        );
        reply.run_id.push('a');
        let mut rejected_output = Vec::new();
        assert_eq!(
            write_frame(&mut rejected_output, &reply)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(rejected_output.is_empty());
    }

    #[test]
    fn reads_fragmented_headers_and_bodies_without_eating_the_next_frame() {
        struct Fragmented {
            input: Cursor<Vec<u8>>,
            size: usize,
        }
        impl io::Read for Fragmented {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                let len = bytes.len().min(self.size);
                io::Read::read(&mut self.input, &mut bytes[..len])
            }
        }
        for size in [1, 2] {
            let reply = Reply {
                run_id: "fragmented".into(),
                status: "pending".into(),
            };
            let mut wire = Vec::new();
            write_frame(&mut wire, &reply).unwrap();
            let first_len = wire.len();
            write_frame(&mut wire, &reply).unwrap();
            let mut reader = Fragmented {
                input: Cursor::new(wire),
                size,
            };
            assert_eq!(read_frame::<_, Reply>(&mut reader).unwrap(), reply);
            assert_eq!(reader.input.position() as usize, first_len);
            assert_eq!(read_frame::<_, Reply>(&mut reader).unwrap(), reply);
            assert_eq!(
                reader.input.position() as usize,
                reader.input.get_ref().len()
            );
        }
    }

    #[test]
    fn write_zero_is_terminal_instead_of_retrying_forever() {
        struct Zero {
            calls: usize,
        }
        impl Write for Zero {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.calls += 1;
                assert_eq!(self.calls, 1, "WriteZero ne doit pas être répété");
                Ok(0)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = Zero { calls: 0 };
        assert_eq!(
            write_frame(&mut writer, &"synthetic").unwrap_err().kind(),
            io::ErrorKind::WriteZero
        );
        assert_eq!(writer.calls, 1);
    }

    #[test]
    fn serialization_failure_after_a_prefix_keeps_the_writer_unchanged() {
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::{Error, SerializeMap};
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("prefix", "synthetic")?;
                Err(S::Error::custom("synthetic serialization failure"))
            }
        }
        let original = b"existing-bytes".to_vec();
        let mut writer = original.clone();
        assert_eq!(
            write_frame(&mut writer, &Broken).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(writer, original);
    }

    #[test]
    fn write_errors_propagate_even_after_partial_header_or_body() {
        struct FailAfter {
            remaining: usize,
        }
        impl Write for FailAfter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.remaining == 0 {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                let written = self.remaining.min(bytes.len()).min(2);
                self.remaining -= written;
                Ok(written)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let reply = Reply {
            run_id: "r".into(),
            status: "ok".into(),
        };
        for limit in [0, 2, 4, 7] {
            assert_eq!(
                write_frame(&mut FailAfter { remaining: limit }, &reply)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::BrokenPipe
            );
        }
    }
}
