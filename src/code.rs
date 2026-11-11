//! The `Code` type: a parsed G/M/T-code.

use crate::code_parameter::CodeParameter;
use crate::types::{CodeChannel, CodeFlags, CodeType, KeywordType, Message};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A parsed representation of a generic G/M/T-code.
///
/// Deserialization is lenient: missing fields get defaults and unknown fields are ignored.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Code {
    /// Connection ID the code was received from (0 for internal DCS tasks).
    pub source_connection: i64,
    /// Result of the code (only set once it has finished).
    pub result: Option<Vec<Message>>,
    /// Type of the code.
    #[serde(rename = "type")]
    pub code_type: CodeType,
    /// Channel to send the code to.
    pub channel: CodeChannel,
    /// Line number.
    pub line_number: Option<i64>,
    /// Number of whitespaces prefixing the code.
    pub indent: u32,
    /// Conditional G-code keyword.
    pub keyword: KeywordType,
    /// Argument of the keyword.
    pub keyword_argument: Option<String>,
    /// Major number (e.g. 28 in G28).
    pub major_number: Option<i32>,
    /// Minor number (e.g. 3 in G54.3).
    pub minor_number: Option<i32>,
    /// Flags.
    pub flags: CodeFlags,
    /// Comment (segments concatenated by the parser).
    pub comment: Option<String>,
    /// File position in bytes.
    pub file_position: Option<i64>,
    /// Length of the original code in bytes.
    pub length: Option<i64>,
    /// Parameters.
    pub parameters: Vec<CodeParameter>,
}

impl Code {
    /// Whether the code comes from a file channel.
    pub fn is_from_file_channel(&self) -> bool {
        matches!(self.channel, CodeChannel::File | CodeChannel::File2)
    }

    /// Find a parameter by letter (case-insensitive).
    pub fn parameter(&self, letter: char) -> Option<&CodeParameter> {
        let l = letter.to_ascii_uppercase();
        self.parameters
            .iter()
            .find(|p| p.letter.to_ascii_uppercase() == l)
    }

    /// Whether the given flag(s) are set.
    pub fn is_flag_set(&self, flag: CodeFlags) -> bool {
        self.flags.0 & flag.0 != 0
    }

    /// Reconstruct an unprecedented string from the parameter list.
    pub fn get_unprecedented_string(&self, quote: bool) -> String {
        self.parameters
            .iter()
            .map(|p| {
                if quote && p.is_string {
                    format!("{}\"{}\"", p.letter, p.string_value)
                } else {
                    format!("{}{}", p.letter, p.string_value)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Only the command portion (e.g. `G28`).
    pub fn short_str(&self) -> String {
        if self.code_type == CodeType::Comment {
            return "(comment)".to_string();
        }
        let prefix = if self.is_flag_set(CodeFlags::ENFORCE_ABSOLUTE_POSITION) {
            "G53 "
        } else {
            ""
        };
        let c = if self.code_type == CodeType::None {
            String::new()
        } else {
            self.code_type.as_char().to_string()
        };
        match (self.major_number, self.minor_number) {
            (Some(maj), Some(min)) => format!("{prefix}{c}{maj}.{min}"),
            (Some(maj), None) => format!("{prefix}{c}{maj}"),
            _ => format!("{prefix}{c}"),
        }
    }

    fn keyword_str(&self) -> &'static str {
        match self.keyword {
            KeywordType::If => "if",
            KeywordType::ElseIf => "elif",
            KeywordType::Else => "else",
            KeywordType::While => "while",
            KeywordType::Break => "break",
            KeywordType::Continue => "continue",
            KeywordType::Abort => "abort",
            KeywordType::Var => "var",
            KeywordType::Set => "set",
            KeywordType::Echo => "echo",
            KeywordType::Global => "global",
            KeywordType::None => "",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.keyword != KeywordType::None {
            return match &self.keyword_argument {
                Some(arg) => write!(f, "{} {}", self.keyword_str(), arg),
                None => write!(f, "{}", self.keyword_str()),
            };
        }
        if self.code_type == CodeType::Comment {
            return write!(f, ";{}", self.comment.as_deref().unwrap_or(""));
        }
        write!(f, "{}", self.short_str())?;
        for p in &self.parameters {
            write!(f, " {p}")?;
        }
        if let Some(c) = self.comment.as_deref().filter(|c| !c.is_empty()) {
            write!(f, " ;{c}")?;
        }
        if let Some(r) = self.result.as_ref().filter(|r| !r.is_empty()) {
            let parts: Vec<String> = r.iter().map(|m| m.to_string()).collect();
            write!(f, " => [{}]", parts.join(", "))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessageType;
    use serde_json::json;

    fn sample() -> Code {
        serde_json::from_value(json!({
            "sourceConnection": 7,
            "result": null,
            "type": "G",
            "channel": "File",
            "lineNumber": 12,
            "indent": 0,
            "keyword": 0,
            "keywordArgument": null,
            "majorNumber": 1,
            "minorNumber": null,
            "flags": 2056,
            "comment": "move",
            "filePosition": 100,
            "length": 20,
            "parameters": [
                {"letter":"X","value":"10.5","isString":false,"isDriverId":false},
                {"letter":"Y","value":"2","isString":false,"isDriverId":false}
            ],
            "someFutureField": {"a": 1}
        }))
        .unwrap()
    }

    #[test]
    fn deserializes_realistic_code() {
        let c = sample();
        assert_eq!(c.source_connection, 7);
        assert_eq!(c.code_type, CodeType::GCode);
        assert_eq!(c.channel, CodeChannel::File);
        assert!(c.is_from_file_channel());
        assert_eq!(c.line_number, Some(12));
        assert_eq!(c.major_number, Some(1));
        assert_eq!(c.minor_number, None);
        assert!(c.is_flag_set(CodeFlags::IS_FROM_MACRO));
        assert!(c.is_flag_set(CodeFlags::IS_LAST_CODE));
        assert!(!c.is_flag_set(CodeFlags::ASYNCHRONOUS));
        assert_eq!(c.parameter('x').unwrap().as_f64().unwrap(), 10.5);
        assert!(c.parameter('Z').is_none());
        assert_eq!(c.to_string(), "G1 X10.5 Y2 ;move");
        assert_eq!(c.get_unprecedented_string(false), "X10.5 Y2");
    }

    #[test]
    fn lenient_minimal() {
        let c: Code =
            serde_json::from_str(r#"{"type":"M","majorNumber":1234,"channel":"Nope"}"#).unwrap();
        assert_eq!(c.code_type, CodeType::MCode);
        assert_eq!(c.channel, CodeChannel::Unknown);
        assert_eq!(c.short_str(), "M1234");
    }

    #[test]
    fn display_variants() {
        let mut c = Code {
            code_type: CodeType::MCode,
            major_number: Some(5),
            minor_number: Some(3),
            flags: CodeFlags::ENFORCE_ABSOLUTE_POSITION,
            ..Code::default()
        };
        assert_eq!(c.to_string(), "G53 M5.3");
        c.result = Some(vec![Message::new(MessageType::Error, "bad")]);
        assert_eq!(c.to_string(), "G53 M5.3 => [Error: bad]");

        let kw = Code {
            keyword: KeywordType::ElseIf,
            keyword_argument: Some("a > 1".into()),
            ..Code::default()
        };
        assert_eq!(kw.to_string(), "elif a > 1");
        let kw = Code {
            keyword: KeywordType::Break,
            ..Code::default()
        };
        assert_eq!(kw.to_string(), "break");
        let cm = Code {
            code_type: CodeType::Comment,
            comment: Some("hi".into()),
            ..Code::default()
        };
        assert_eq!(cm.to_string(), ";hi");
        assert_eq!(cm.short_str(), "(comment)");
    }
}
