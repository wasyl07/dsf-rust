//! Enumerations and small value types shared by commands and codes.

use crate::error::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt;
use std::ops::{BitOr, BitOrAssign};
use std::str::FromStr;

/// Code channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CodeChannel {
    /// HTTP requests.
    #[serde(rename = "HTTP")]
    Http,
    /// Telnet requests.
    Telnet,
    /// Primary file prints.
    File,
    /// USB requests.
    #[serde(rename = "USB")]
    Usb,
    /// Serial devices (e.g. PanelDue).
    Aux,
    /// Triggers or config.g.
    Trigger,
    /// Code queue running in sync with the primary print file.
    Queue,
    /// Auxiliary LCD devices.
    #[serde(rename = "LCD")]
    Lcd,
    /// Default channel for requests over SPI.
    #[default]
    #[serde(rename = "SBC")]
    Sbc,
    /// Daemon process.
    Daemon,
    /// Second UART port.
    Aux2,
    /// Power fail, heater fault and filament out macros.
    Autopause,
    /// Secondary file prints.
    File2,
    /// Code queue for the secondary print file.
    Queue2,
    /// Unknown channel (also used for channel names this client does not know).
    #[serde(other)]
    Unknown,
}

impl CodeChannel {
    /// Every channel, including `Unknown`.
    pub const ALL: [CodeChannel; 15] = [
        CodeChannel::Http,
        CodeChannel::Telnet,
        CodeChannel::File,
        CodeChannel::Usb,
        CodeChannel::Aux,
        CodeChannel::Trigger,
        CodeChannel::Queue,
        CodeChannel::Lcd,
        CodeChannel::Sbc,
        CodeChannel::Daemon,
        CodeChannel::Aux2,
        CodeChannel::Autopause,
        CodeChannel::File2,
        CodeChannel::Queue2,
        CodeChannel::Unknown,
    ];
}

/// Type of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum MessageType {
    /// Success.
    #[default]
    Success = 0,
    /// Warning.
    Warning = 1,
    /// Error.
    Error = 2,
}

/// Log level of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Debug.
    Debug,
    /// Info.
    Info,
    /// Warn.
    Warn,
    /// Logging off.
    Off,
}

/// Type of an HTTP endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HttpEndpointType {
    /// GET.
    #[serde(rename = "GET")]
    Get,
    /// POST.
    #[serde(rename = "POST")]
    Post,
    /// PUT.
    #[serde(rename = "PUT")]
    Put,
    /// PATCH.
    #[serde(rename = "PATCH")]
    Patch,
    /// TRACE.
    #[serde(rename = "TRACE")]
    Trace,
    /// DELETE.
    #[serde(rename = "DELETE")]
    Delete,
    /// OPTIONS.
    #[serde(rename = "OPTIONS")]
    Options,
    /// WebSocket.
    WebSocket,
}

/// Access level of a user session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessLevel {
    /// Read-only access.
    ReadOnly,
    /// Read-write access.
    ReadWrite,
}

/// Type of a user session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    /// Local session.
    Local,
    /// HTTP session.
    Http,
    /// Telnet session.
    Telnet,
}

/// Interception mode of an intercept connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InterceptionMode {
    /// Before the code is processed.
    Pre,
    /// After the code was processed internally.
    Post,
    /// After the code was executed by the firmware.
    Executed,
}

/// Subscription mode of a subscribe connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubscriptionMode {
    /// Full object model on every update.
    Full,
    /// Full model once, then patches.
    Patch,
}

/// Type of a G/M/T-code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CodeType {
    /// Undetermined.
    #[default]
    #[serde(rename = "\u{0}")]
    None,
    /// Whole-line comment.
    #[serde(rename = "Q")]
    Comment,
    /// Meta G-code keyword.
    #[serde(rename = "K")]
    Keyword,
    /// G-code.
    #[serde(rename = "G")]
    GCode,
    /// M-code.
    #[serde(rename = "M")]
    MCode,
    /// T-code.
    #[serde(rename = "T")]
    TCode,
}

impl CodeType {
    /// The wire character of this type.
    pub fn as_char(self) -> char {
        match self {
            CodeType::None => '\0',
            CodeType::Comment => 'Q',
            CodeType::Keyword => 'K',
            CodeType::GCode => 'G',
            CodeType::MCode => 'M',
            CodeType::TCode => 'T',
        }
    }
}

/// Conditional G-code keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum KeywordType {
    /// No keyword.
    #[default]
    None = 0,
    /// `if`
    If = 1,
    /// `elif`
    ElseIf = 2,
    /// `else`
    Else = 3,
    /// `while`
    While = 4,
    /// `break`
    Break = 5,
    /// `abort`
    Abort = 6,
    /// `var`
    Var = 7,
    /// `set`
    Set = 8,
    /// `echo`
    Echo = 9,
    /// `continue`
    Continue = 10,
    /// `global`
    Global = 11,
}

/// Bit flags of a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CodeFlags(pub u32);

impl CodeFlags {
    /// No flags.
    pub const NONE: CodeFlags = CodeFlags(0);
    /// Code may be executed asynchronously.
    pub const ASYNCHRONOUS: CodeFlags = CodeFlags(1);
    /// Code was pre-processed.
    pub const IS_PRE_PROCESSED: CodeFlags = CodeFlags(2);
    /// Code was post-processed.
    pub const IS_POST_PROCESSED: CodeFlags = CodeFlags(4);
    /// Code is from a macro.
    pub const IS_FROM_MACRO: CodeFlags = CodeFlags(8);
    /// Code is from a nested macro.
    pub const IS_NESTED_MACRO: CodeFlags = CodeFlags(16);
    /// Code is from config.g.
    pub const IS_FROM_CONFIG: CodeFlags = CodeFlags(32);
    /// Code is from config-override.g.
    pub const IS_FROM_CONFIG_OVERRIDE: CodeFlags = CodeFlags(64);
    /// Enforce absolute positioning (G53).
    pub const ENFORCE_ABSOLUTE_POSITION: CodeFlags = CodeFlags(128);
    /// Code is prioritized.
    pub const IS_PRIORITIZED: CodeFlags = CodeFlags(256);
    /// Code is unbuffered.
    pub const UNBUFFERED: CodeFlags = CodeFlags(512);
    /// Code is from the firmware.
    pub const IS_FROM_FIRMWARE: CodeFlags = CodeFlags(1024);
    /// Code is the last of its line/file.
    pub const IS_LAST_CODE: CodeFlags = CodeFlags(2048);
    /// Code is processed internally.
    pub const IS_INTERNALLY_PROCESSED: CodeFlags = CodeFlags(4096);

    /// Whether all bits of `other` are set in `self`.
    pub fn contains(self, other: CodeFlags) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for CodeFlags {
    type Output = CodeFlags;
    fn bitor(self, rhs: CodeFlags) -> CodeFlags {
        CodeFlags(self.0 | rhs.0)
    }
}

impl BitOrAssign for CodeFlags {
    fn bitor_assign(&mut self, rhs: CodeFlags) {
        self.0 |= rhs.0;
    }
}

/// A message from the machine.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Message {
    /// Message type.
    #[serde(rename = "type")]
    pub message_type: MessageType,
    /// Message content.
    pub content: String,
    /// Time of the message as reported by the server (not parsed).
    pub time: Option<String>,
}

impl Message {
    /// Create a message without a timestamp.
    pub fn new(message_type: MessageType, content: impl Into<String>) -> Self {
        Message {
            message_type,
            content: content.into(),
            time: None,
        }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.message_type {
            MessageType::Error => write!(f, "Error: {}", self.content),
            MessageType::Warning => write!(f, "Warning: {}", self.content),
            MessageType::Success => write!(f, "{}", self.content),
        }
    }
}

/// A driver identifier (board and port).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DriverId {
    /// Board number.
    pub board: u16,
    /// Port number.
    pub port: u16,
}

impl DriverId {
    /// Create a driver id.
    pub fn new(board: u16, port: u16) -> Self {
        DriverId { board, port }
    }

    /// Combined integer representation (`board << 16 | port`).
    pub fn as_u32(self) -> u32 {
        (u32::from(self.board) << 16) | u32::from(self.port)
    }
}

impl From<u32> for DriverId {
    fn from(v: u32) -> Self {
        DriverId {
            board: (v >> 16) as u16,
            port: (v & 0xFFFF) as u16,
        }
    }
}

impl FromStr for DriverId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bad = || Error::CodeParser(format!("Failed to parse driver value '{s}'"));
        let num = |p: &str| p.trim().parse::<i64>().map_err(|_| bad());
        let segments: Vec<&str> = s.split('.').collect();
        match segments.as_slice() {
            [port] => Ok(DriverId::new(0, (num(port)? & 0xFFFF) as u16)),
            [board, port] => Ok(DriverId::new(
                (num(board)? & 0xFFFF) as u16,
                (num(port)? & 0xFFFF) as u16,
            )),
            _ => Err(bad()),
        }
    }
}

impl fmt::Display for DriverId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.board, self.port)
    }
}

impl Serialize for DriverId {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DriverId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enum_wire_values() {
        assert_eq!(
            serde_json::to_value(CodeChannel::Sbc).unwrap(),
            json!("SBC")
        );
        assert_eq!(
            serde_json::to_value(CodeChannel::Http).unwrap(),
            json!("HTTP")
        );
        assert_eq!(
            serde_json::to_value(CodeChannel::Aux2).unwrap(),
            json!("Aux2")
        );
        assert_eq!(serde_json::to_value(MessageType::Error).unwrap(), json!(2));
        assert_eq!(serde_json::to_value(LogLevel::Warn).unwrap(), json!("warn"));
        assert_eq!(
            serde_json::to_value(HttpEndpointType::WebSocket).unwrap(),
            json!("WebSocket")
        );
        assert_eq!(
            serde_json::to_value(HttpEndpointType::Get).unwrap(),
            json!("GET")
        );
        assert_eq!(
            serde_json::to_value(AccessLevel::ReadOnly).unwrap(),
            json!("readOnly")
        );
        assert_eq!(
            serde_json::to_value(AccessLevel::ReadWrite).unwrap(),
            json!("readWrite")
        );
        assert_eq!(
            serde_json::to_value(SessionType::Telnet).unwrap(),
            json!("telnet")
        );
        assert_eq!(
            serde_json::to_value(CodeType::None).unwrap(),
            json!("\u{0}")
        );
        assert_eq!(serde_json::to_value(CodeType::MCode).unwrap(), json!("M"));
        assert_eq!(
            serde_json::to_value(KeywordType::Global).unwrap(),
            json!(11)
        );
        assert_eq!(
            serde_json::to_value(CodeFlags::IS_LAST_CODE).unwrap(),
            json!(2048)
        );
    }

    #[test]
    fn unknown_channel_is_tolerated() {
        let c: CodeChannel = serde_json::from_value(json!("Whatever")).unwrap();
        assert_eq!(c, CodeChannel::Unknown);
        let c: CodeChannel = serde_json::from_value(json!("File2")).unwrap();
        assert_eq!(c, CodeChannel::File2);
    }

    #[test]
    fn code_flags() {
        let f = CodeFlags::ASYNCHRONOUS | CodeFlags::IS_FROM_MACRO;
        assert_eq!(f.0, 9);
        assert!(f.contains(CodeFlags::IS_FROM_MACRO));
        assert!(!f.contains(CodeFlags::IS_LAST_CODE));
    }

    #[test]
    fn message_display() {
        assert_eq!(
            Message::new(MessageType::Error, "x").to_string(),
            "Error: x"
        );
        assert_eq!(
            Message::new(MessageType::Warning, "x").to_string(),
            "Warning: x"
        );
        assert_eq!(Message::new(MessageType::Success, "x").to_string(), "x");
        let m: Message =
            serde_json::from_value(json!({"type":1,"content":"c","time":"2024-01-01T00:00:00"}))
                .unwrap();
        assert_eq!(m.message_type, MessageType::Warning);
        assert_eq!(m.time.as_deref(), Some("2024-01-01T00:00:00"));
    }

    #[test]
    fn driver_id() {
        assert_eq!("3".parse::<DriverId>().unwrap(), DriverId::new(0, 3));
        assert_eq!("1.2".parse::<DriverId>().unwrap(), DriverId::new(1, 2));
        assert!("1.2.3".parse::<DriverId>().is_err());
        assert!("a".parse::<DriverId>().is_err());
        let d = DriverId::from(0x0001_0002u32);
        assert_eq!(d, DriverId::new(1, 2));
        assert_eq!(d.as_u32(), 0x0001_0002);
        assert_eq!(d.to_string(), "1.2");
        assert_eq!(serde_json::to_value(d).unwrap(), json!("1.2"));
        let back: DriverId = serde_json::from_value(json!("1.2")).unwrap();
        assert_eq!(back, d);
    }
}
