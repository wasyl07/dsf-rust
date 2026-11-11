//! Parsed code parameters.

use crate::error::{Error, Result};
use crate::types::DriverId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Letter used for parameters that carry an unprecedented string.
pub const LETTER_FOR_UNPRECEDENTED_STRING: char = '@';

/// A parsed parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterValue {
    /// Integer.
    Int(i64),
    /// Floating point number.
    Float(f64),
    /// String (also the fallback when nothing else parses).
    Str(String),
    /// Colon-separated integers.
    IntArray(Vec<i64>),
    /// Colon-separated floats.
    FloatArray(Vec<f64>),
    /// A driver id.
    DriverId(DriverId),
    /// Colon-separated driver ids.
    DriverIdArray(Vec<DriverId>),
    /// A `{...}` expression.
    Expression(String),
}

/// A parameter of a G/M/T-code.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeParameter {
    /// Parameter letter.
    pub letter: char,
    /// Raw string value as sent by the server.
    pub string_value: String,
    /// Whether the value is a string.
    pub is_string: bool,
    /// Whether the value is a driver id (or list of driver ids).
    pub is_driver_id: bool,
    /// Parsed value.
    pub value: ParameterValue,
}

impl CodeParameter {
    /// Create a parameter from its raw wire representation and parse the value.
    pub fn new(
        letter: char,
        string_value: impl Into<String>,
        is_string: bool,
        is_driver_id: bool,
    ) -> Self {
        let string_value = string_value.into();
        let value = parse_value(&string_value, is_string, is_driver_id);
        CodeParameter {
            letter,
            string_value,
            is_string,
            is_driver_id,
            value,
        }
    }

    /// Create a non-string parameter from any displayable value (e.g. a number).
    pub fn from_value(letter: char, value: impl ToString) -> Self {
        Self::new(letter, value.to_string(), false, false)
    }

    /// Whether the value is an expression.
    pub fn is_expression(&self) -> bool {
        matches!(self.value, ParameterValue::Expression(_))
    }

    fn fail<T>(&self, what: &str) -> Result<T> {
        Err(Error::CodeParser(format!(
            "Cannot convert {} parameter to {} (value {})",
            self.letter, what, self.string_value
        )))
    }

    /// Convert to `f64`.
    pub fn as_f64(&self) -> Result<f64> {
        match self.value {
            ParameterValue::Float(f) => Ok(f),
            ParameterValue::Int(i) => Ok(i as f64),
            _ => self.fail("float"),
        }
    }

    /// Convert to `i64`.
    pub fn as_i64(&self) -> Result<i64> {
        match self.value {
            ParameterValue::Int(i) => Ok(i),
            ParameterValue::DriverId(d) => Ok(i64::from(d.as_u32())),
            _ => self.fail("int"),
        }
    }

    /// Convert to a driver id.
    pub fn as_driver_id(&self) -> Result<DriverId> {
        match self.value {
            ParameterValue::DriverId(d) => Ok(d),
            ParameterValue::Int(i) if (0..=i64::from(u32::MAX)).contains(&i) => {
                Ok(DriverId::from(i as u32))
            }
            _ => self.fail("DriverId"),
        }
    }

    /// Convert to a vector of `f64`.
    pub fn as_f64_vec(&self) -> Result<Vec<f64>> {
        match &self.value {
            ParameterValue::FloatArray(v) => Ok(v.clone()),
            ParameterValue::IntArray(v) => Ok(v.iter().map(|&i| i as f64).collect()),
            ParameterValue::Float(f) => Ok(vec![*f]),
            ParameterValue::Int(i) => Ok(vec![*i as f64]),
            _ => self.fail("float array"),
        }
    }

    /// Convert to a vector of `i64` (floats are truncated).
    pub fn as_i64_vec(&self) -> Result<Vec<i64>> {
        match &self.value {
            ParameterValue::IntArray(v) => Ok(v.clone()),
            ParameterValue::FloatArray(v) => Ok(v.iter().map(|&f| f as i64).collect()),
            ParameterValue::DriverIdArray(v) => {
                Ok(v.iter().map(|d| i64::from(d.as_u32())).collect())
            }
            ParameterValue::Int(i) => Ok(vec![*i]),
            ParameterValue::DriverId(d) => Ok(vec![i64::from(d.as_u32())]),
            _ => self.fail("int array"),
        }
    }

    /// Convert to a vector of driver ids.
    pub fn as_driver_id_vec(&self) -> Result<Vec<DriverId>> {
        match &self.value {
            ParameterValue::DriverIdArray(v) => Ok(v.clone()),
            ParameterValue::DriverId(d) => Ok(vec![*d]),
            ParameterValue::IntArray(v)
                if v.iter().all(|i| (0..=i64::from(u32::MAX)).contains(i)) =>
            {
                Ok(v.iter().map(|&i| DriverId::from(i as u32)).collect())
            }
            ParameterValue::Int(i) if (0..=i64::from(u32::MAX)).contains(i) => {
                Ok(vec![DriverId::from(*i as u32)])
            }
            _ => self.fail("DriverId array"),
        }
    }

    /// Convert to bool: the raw value parsed as a float must be greater than zero.
    pub fn as_bool(&self) -> bool {
        self.string_value
            .trim()
            .parse::<f64>()
            .map(|f| f > 0.0)
            .unwrap_or(false)
    }
}

fn parse_driver_ids(value: &str) -> Option<ParameterValue> {
    let drivers: Option<Vec<DriverId>> = value.split(':').map(|s| s.parse().ok()).collect();
    let mut drivers = drivers?;
    if drivers.len() == 1 {
        Some(ParameterValue::DriverId(drivers.remove(0)))
    } else {
        Some(ParameterValue::DriverIdArray(drivers))
    }
}

fn parse_value(raw: &str, is_string: bool, is_driver_id: bool) -> ParameterValue {
    if is_string {
        return ParameterValue::Str(raw.to_string());
    }
    if is_driver_id {
        if let Some(v) = parse_driver_ids(raw) {
            return v;
        }
    }
    let value = raw.trim();
    if value.is_empty() {
        // Empty parameters are integers with value 0 (e.g. G92 XY => G92 X0 Y0)
        ParameterValue::Int(0)
    } else if value.starts_with('{') && value.ends_with('}') {
        ParameterValue::Expression(value.to_string())
    } else if value.contains(':') {
        let parts = value.split(':');
        if value.contains('.') {
            let floats: std::result::Result<Vec<f64>, _> =
                parts.map(|p| p.trim().parse()).collect();
            floats.map(ParameterValue::FloatArray).map_err(|_| ())
        } else {
            let ints: std::result::Result<Vec<i64>, _> = parts.map(|p| p.trim().parse()).collect();
            ints.map(ParameterValue::IntArray).map_err(|_| ())
        }
        .unwrap_or_else(|_| ParameterValue::Str(value.to_string()))
    } else if let Ok(i) = value.parse::<i64>() {
        ParameterValue::Int(i)
    } else if let Ok(f) = value.parse::<f64>() {
        ParameterValue::Float(f)
    } else {
        ParameterValue::Str(value.to_string())
    }
}

impl fmt::Display for CodeParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.letter != LETTER_FOR_UNPRECEDENTED_STRING {
            write!(f, "{}", self.letter)?;
        }
        if self.is_string && !self.is_expression() {
            write!(f, "\"{}\"", self.string_value.replace('"', "\"\""))
        } else {
            write!(f, "{}", self.string_value)
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    letter: String,
    value: String,
    #[serde(default)]
    is_string: bool,
    #[serde(default)]
    is_driver_id: bool,
}

impl Serialize for CodeParameter {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        Wire {
            letter: self.letter.to_string(),
            value: self.string_value.clone(),
            is_string: self.is_string,
            is_driver_id: self.is_driver_id,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for CodeParameter {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let w = Wire::deserialize(d)?;
        let letter = w
            .letter
            .chars()
            .next()
            .unwrap_or(LETTER_FOR_UNPRECEDENTED_STRING);
        Ok(CodeParameter::new(
            letter,
            w.value,
            w.is_string,
            w.is_driver_id,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(v: &str) -> CodeParameter {
        CodeParameter::new('X', v, false, false)
    }

    #[test]
    fn parsing_rules() {
        assert_eq!(
            CodeParameter::new('S', "a b", true, false).value,
            ParameterValue::Str("a b".into())
        );
        assert_eq!(p("").value, ParameterValue::Int(0));
        assert_eq!(p("  ").value, ParameterValue::Int(0));
        assert_eq!(p("{1+2}").value, ParameterValue::Expression("{1+2}".into()));
        assert_eq!(p("1:2:3").value, ParameterValue::IntArray(vec![1, 2, 3]));
        assert_eq!(p("1:2.5").value, ParameterValue::FloatArray(vec![1.0, 2.5]));
        assert_eq!(p("1:a").value, ParameterValue::Str("1:a".into()));
        assert_eq!(p("1.5:a").value, ParameterValue::Str("1.5:a".into()));
        assert_eq!(p("42").value, ParameterValue::Int(42));
        assert_eq!(p("-3").value, ParameterValue::Int(-3));
        assert_eq!(p("1.5").value, ParameterValue::Float(1.5));
        assert_eq!(p("abc").value, ParameterValue::Str("abc".into()));
        assert_eq!(
            CodeParameter::new('P', "1.2", false, true).value,
            ParameterValue::DriverId(DriverId::new(1, 2))
        );
        assert_eq!(
            CodeParameter::new('P', "0:1.2", false, true).value,
            ParameterValue::DriverIdArray(vec![DriverId::new(0, 0), DriverId::new(1, 2)])
        );
    }

    #[test]
    fn accessors() {
        assert_eq!(p("3").as_f64().unwrap(), 3.0);
        assert_eq!(p("3").as_i64().unwrap(), 3);
        assert!(p("3.5").as_i64().is_err());
        assert!(p("abc").as_f64().is_err());
        assert_eq!(p("3").as_driver_id().unwrap(), DriverId::new(0, 3));
        assert_eq!(p("1:2").as_i64_vec().unwrap(), vec![1, 2]);
        assert_eq!(p("1:2").as_f64_vec().unwrap(), vec![1.0, 2.0]);
        assert_eq!(p("4").as_f64_vec().unwrap(), vec![4.0]);
        assert_eq!(
            p("1:2").as_driver_id_vec().unwrap(),
            vec![DriverId::new(0, 1), DriverId::new(0, 2)]
        );
        let d = CodeParameter::new('P', "1.2", false, true);
        assert_eq!(d.as_i64().unwrap(), 0x0001_0002);
        assert_eq!(d.as_driver_id_vec().unwrap(), vec![DriverId::new(1, 2)]);
        assert!(p("1").as_bool());
        assert!(!p("0").as_bool());
        assert!(!p("abc").as_bool());
    }

    #[test]
    fn display() {
        assert_eq!(p("5").to_string(), "X5");
        assert_eq!(
            CodeParameter::new('S', "say \"hi\"", true, false).to_string(),
            "S\"say \"\"hi\"\"\""
        );
        assert_eq!(
            CodeParameter::new('@', "hello", true, false).to_string(),
            "\"hello\""
        );
        assert_eq!(
            CodeParameter::new('@', "hello", false, false).to_string(),
            "hello"
        );
    }

    #[test]
    fn serde_wire_format() {
        let v = serde_json::to_value(CodeParameter::new('S', "x", true, false)).unwrap();
        assert_eq!(
            v,
            json!({"letter":"S","value":"x","isString":true,"isDriverId":false})
        );
        let c: CodeParameter = serde_json::from_value(
            json!({"letter":"P","value":"1.2","isString":false,"isDriverId":true}),
        )
        .unwrap();
        assert_eq!(c.value, ParameterValue::DriverId(DriverId::new(1, 2)));
        assert_eq!(c.letter, 'P');
    }
}
