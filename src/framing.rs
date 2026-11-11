//! Extraction of delimiter-less JSON objects from a byte stream.

use crate::error::{Error, Result};
use serde::de::IgnoredAny;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Poll interval used to check the cancel flag while waiting for data.
pub(crate) const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Take the next complete JSON value off the front of `buf`.
///
/// Returns `Ok(None)` if the buffer does not (yet) hold a complete value. Leftover bytes
/// stay in the buffer. This uses a real JSON parser, so braces inside strings are fine.
pub(crate) fn take_frame(buf: &mut Vec<u8>) -> Result<Option<String>> {
    let start = buf
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(buf.len());
    if start == buf.len() {
        buf.clear();
        return Ok(None);
    }
    let mut it = serde_json::Deserializer::from_slice(&buf[start..]).into_iter::<IgnoredAny>();
    match it.next() {
        Some(Ok(_)) => {
            let end = start + it.byte_offset();
            let frame = String::from_utf8(buf[start..end].to_vec())
                .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
            buf.drain(..end);
            Ok(Some(frame))
        }
        Some(Err(e)) if e.is_eof() => Ok(None),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Read one JSON value from `stream`, using and refilling `buf`.
///
/// `timeout` is the overall time allowed for this call. If `cancel` is given, the socket is
/// polled and the flag is checked between reads (after the buffer, so a complete buffered
/// frame wins); a set flag is reset and yields [`Error::Cancelled`].
pub(crate) fn read_frame(
    stream: &UnixStream,
    buf: &mut Vec<u8>,
    timeout: Option<Duration>,
    cancel: Option<&AtomicBool>,
) -> Result<String> {
    let deadline = timeout.map(|t| Instant::now() + t);
    let mut reader = stream;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        if let Some(frame) = take_frame(buf)? {
            return Ok(frame);
        }
        if let Some(c) = cancel {
            if c.swap(false, Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }
        }
        let remaining = match deadline {
            Some(d) => {
                let now = Instant::now();
                if now >= d {
                    return Err(Error::Timeout);
                }
                Some(d - now)
            }
            None => None,
        };
        let remaining = match (cancel, remaining) {
            (Some(_), Some(r)) => Some(r.min(CANCEL_POLL_INTERVAL)),
            (Some(_), None) => Some(CANCEL_POLL_INTERVAL),
            (None, r) => r,
        };
        stream.set_read_timeout(remaining)?;
        match reader.read(&mut chunk) {
            Ok(0) => {
                return if buf.iter().all(|b| b.is_ascii_whitespace()) {
                    Err(Error::ConnectionClosed)
                } else {
                    Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into()))
                };
            }
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if cancel.is_none() {
                    return Err(Error::Timeout);
                }
                // Poll expired: loop to check the cancel flag and the overall deadline.
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_objects_in_one_buffer() {
        let mut b = br#"{"a":1}{"b":2}"#.to_vec();
        assert_eq!(take_frame(&mut b).unwrap().unwrap(), r#"{"a":1}"#);
        assert_eq!(take_frame(&mut b).unwrap().unwrap(), r#"{"b":2}"#);
        assert!(take_frame(&mut b).unwrap().is_none());
        assert!(b.is_empty());
    }

    #[test]
    fn split_across_reads() {
        let full = br#"{"a":[1,2,{"x":"y"}]}"#;
        let mut b = Vec::new();
        for (i, byte) in full.iter().enumerate() {
            b.push(*byte);
            let r = take_frame(&mut b).unwrap();
            if i + 1 < full.len() {
                assert!(r.is_none(), "complete too early at {i}");
            } else {
                assert_eq!(r.unwrap().as_bytes(), full);
            }
        }
    }

    #[test]
    fn braces_inside_strings() {
        let mut b = br#"{"s":"}{ \" }"}{"t":"{"}"#.to_vec();
        assert_eq!(take_frame(&mut b).unwrap().unwrap(), r#"{"s":"}{ \" }"}"#);
        assert_eq!(take_frame(&mut b).unwrap().unwrap(), r#"{"t":"{"}"#);
    }

    #[test]
    fn leftover_partial_is_kept() {
        let mut b = br#"{"a":1}{"b""#.to_vec();
        assert_eq!(take_frame(&mut b).unwrap().unwrap(), r#"{"a":1}"#);
        assert!(take_frame(&mut b).unwrap().is_none());
        assert_eq!(b, br#"{"b""#.to_vec());
    }

    #[test]
    fn garbage_is_an_error() {
        let mut b = b"}".to_vec();
        assert!(take_frame(&mut b).is_err());
    }
}
