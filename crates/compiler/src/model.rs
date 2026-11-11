//! The project of the Rocket Show designer, read from its JSON the way the Java engine reads it.
//!
//! A port of the model classes of `lighting/designer` of Rocket Show, which Jackson reads with
//! `DesignerJson.MAPPER` (Jackson 3): a value which is `null` keeps the default of its field, a
//! property nobody knows is ignored, a name of an enum nobody knows is read as missing, the last of
//! two properties of the same name wins, and scalars are coerced the way Jackson coerces them (a
//! number is read into a text as it is written, `"12"` into a number, `1.9` into the whole number
//! 1, 0 and 1 into a boolean). The setters of the Java classes which replace an empty value by a
//! default are followed as well.
//!
//! The JSON itself is read by [`Json::parse`], not by `serde_json`: the objects keep the order of
//! their names (the pixel groups of a matrix and the channels of a profile are played in it), and
//! the numbers are the very doubles Java reads (`serde_json` only promises them with its
//! `float_roundtrip` feature, which is off). Every type also implements `serde::Deserialize`,
//! through [`Json`], for whoever reads with serde: the order is kept there as well, the numbers
//! are then the ones the serde format hands over.

use std::borrow::Cow;
use std::fmt;

/// The highest project version the Java engine knows (`SUPPORTED_PROJECT_VERSION`). Only projects
/// of this version are compiled, the migrations of the older ones are not ported.
pub const SUPPORTED_PROJECT_VERSION: i32 = 11;

// ---------------------------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------------------------

/// A JSON value. The properties of an object are kept as they are written: in their order, and
/// all of them if a name is written twice (see [`Json::get`] and [`Json::properties`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// A JSON number, kept as it is written (Jackson reads a number into a text field as written).
#[derive(Debug, Clone, PartialEq)]
pub struct JsonNumber {
    raw: String,
}

impl JsonNumber {
    pub fn from_f64(value: f64) -> JsonNumber {
        JsonNumber {
            raw: format!("{value:?}"),
        }
    }

    pub fn from_i64(value: i64) -> JsonNumber {
        JsonNumber { raw: value.to_string() }
    }

    /// The number as it is written.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Written without a fraction and without an exponent.
    pub fn is_integer(&self) -> bool {
        !self.raw.contains(['.', 'e', 'E'])
    }

    /// The double closest to the number, like `Double.parseDouble`.
    pub fn as_f64(&self) -> f64 {
        self.raw.parse::<f64>().unwrap_or(f64::NAN)
    }

    /// The number, if it is written as a whole one and fits.
    pub fn as_i64(&self) -> Option<i64> {
        if self.is_integer() {
            self.raw.parse::<i64>().ok()
        } else {
            None
        }
    }
}

impl Json {
    /// Reads a JSON text, as strictly as Jackson does: no comments, no trailing commas, nothing
    /// after the value. A name written twice in an object is kept twice.
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser {
            text,
            bytes: text.as_bytes(),
            position: 0,
        };
        if text.starts_with('\u{feff}') {
            parser.position = 3;
        }
        parser.skip_whitespace();
        let value = parser.value(0)?;
        parser.skip_whitespace();
        if parser.position != parser.bytes.len() {
            return Err(parser.error("unexpected text after the value"));
        }
        Ok(value)
    }

    pub fn number(value: f64) -> Json {
        Json::Number(JsonNumber::from_f64(value))
    }

    pub fn string(value: impl Into<String>) -> Json {
        Json::String(value.into())
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(number) => Some(number.as_f64()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The properties of an object as they are written (a name written twice comes twice).
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Object(entries) => Some(entries),
            _ => None,
        }
    }

    /// The property of an object, the last one of the name (like a `JsonNode` of Jackson).
    pub fn get(&self, name: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries
                .iter()
                .rev()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// The properties of an object like a `LinkedHashMap` of Java keeps them: each name once, at
    /// the place it is first written at, with the value it is last written with. None for
    /// anything but an object.
    pub fn properties(&self) -> Vec<(&str, &Json)> {
        let mut properties: Vec<(&str, &Json)> = Vec::new();
        if let Json::Object(entries) = self {
            for (key, value) in entries {
                match properties.iter_mut().find(|(name, _)| *name == key.as_str()) {
                    Some(property) => property.1 = value,
                    None => properties.push((key, value)),
                }
            }
        }
        properties
    }

    /// Sets a property of an object: in place of the ones of the name, or else at the end.
    /// Nothing happens on anything but an object.
    pub fn set(&mut self, name: &str, value: Json) {
        if let Json::Object(entries) = self {
            match entries.iter().position(|(key, _)| key == name) {
                Some(first) => {
                    entries[first].1 = value;
                    let mut index = 0;
                    entries.retain(|(key, _)| {
                        index += 1;
                        index - 1 == first || key != name
                    });
                }
                None => entries.push((name.to_string(), value)),
            }
        }
    }

    /// The text of a scalar like `JsonNode.asText()` of Jackson 3 gives it: a text as it is, a
    /// whole number in digits, any other number like `Double.toString`, `true` or `false`, the
    /// empty text for `null`. An array and an object have none (Jackson throws).
    pub fn as_text(&self) -> Result<String, String> {
        match self {
            Json::Null => Ok(String::new()),
            Json::Bool(value) => Ok(value.to_string()),
            Json::Number(number) => Ok(java_number_text(number)),
            Json::String(text) => Ok(text.clone()),
            Json::Array(_) => Err("an array has no text".to_string()),
            Json::Object(_) => Err("an object has no text".to_string()),
        }
    }
}

impl From<&serde_json::Value> for Json {
    /// A value of `serde_json`. Mind that it has lost the order of its names already (unless
    /// `serde_json` is built with `preserve_order`), which the pixel groups of a matrix are
    /// played in: read the text with [`Json::parse`] wherever there is one.
    fn from(value: &serde_json::Value) -> Json {
        match value {
            serde_json::Value::Null => Json::Null,
            serde_json::Value::Bool(value) => Json::Bool(*value),
            serde_json::Value::Number(number) => Json::Number(JsonNumber {
                raw: number.to_string(),
            }),
            serde_json::Value::String(text) => Json::String(text.clone()),
            serde_json::Value::Array(items) => Json::Array(items.iter().map(Json::from).collect()),
            serde_json::Value::Object(entries) => Json::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Json::from(value)))
                    .collect(),
            ),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Json, D::Error> {
        struct JsonVisitor;

        impl<'de> serde::de::Visitor<'de> for JsonVisitor {
            type Value = Json;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Json, E> {
                Ok(Json::Bool(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Json, E> {
                Ok(Json::Number(JsonNumber::from_i64(value)))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Json, E> {
                Ok(Json::Number(JsonNumber { raw: value.to_string() }))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Json, E> {
                if value.is_finite() {
                    Ok(Json::Number(JsonNumber::from_f64(value)))
                } else {
                    Err(E::custom("a number which is not finite"))
                }
            }

            fn visit_str<E>(self, value: &str) -> Result<Json, E> {
                Ok(Json::String(value.to_string()))
            }

            fn visit_string<E>(self, value: String) -> Result<Json, E> {
                Ok(Json::String(value))
            }

            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }

            fn visit_none<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }

            fn visit_some<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Json, D::Error> {
                serde::Deserialize::deserialize(deserializer)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> Result<Json, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = sequence.next_element()? {
                    items.push(item);
                }
                Ok(Json::Array(items))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, Json>()? {
                    entries.push(entry);
                }
                Ok(Json::Object(entries))
            }
        }

        deserializer.deserialize_any(JsonVisitor)
    }
}

// as deep as Jackson reads (StreamReadConstraints)
const MAX_DEPTH: usize = 1000;

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("JSON: {message} at byte {}", self.position)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.position += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
            None => Err(self.error("unexpected end")),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.position..].starts_with(word.as_bytes()) {
            self.position += word.len();
            Ok(value)
        } else {
            Err(self.error("unexpected token"))
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.position;
        while let Some(b'0'..=b'9') = self.peek() {
            self.position += 1;
        }
        self.position - start
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        match self.peek() {
            Some(b'0') => self.position += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.error("a number needs a digit")),
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            if self.digits() == 0 {
                return Err(self.error("a fraction needs a digit"));
            }
        }
        if let Some(b'e' | b'E') = self.peek() {
            self.position += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.position += 1;
            }
            if self.digits() == 0 {
                return Err(self.error("an exponent needs a digit"));
            }
        }
        Ok(Json::Number(JsonNumber {
            raw: self.text[start..self.position].to_string(),
        }))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .bytes
            .get(self.position..self.position + 4)
            .ok_or_else(|| self.error("unexpected end"))?;
        let mut value = 0;
        for digit in digits {
            value = value * 16
                + (*digit as char)
                    .to_digit(16)
                    .ok_or_else(|| self.error("not a hexadecimal digit"))?;
        }
        self.position += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, String> {
        // the opening quote
        self.position += 1;
        let mut result = String::new();
        loop {
            let start = self.position;
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.position += 1;
            }
            result.push_str(&self.text[start..self.position]);
            match self.peek() {
                Some(b'"') => {
                    self.position += 1;
                    return Ok(result);
                }
                Some(b'\\') => {
                    self.position += 1;
                    let escape = self.peek().ok_or_else(|| self.error("unexpected end"))?;
                    self.position += 1;
                    match escape {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        b'/' => result.push('/'),
                        b'b' => result.push('\u{8}'),
                        b'f' => result.push('\u{c}'),
                        b'n' => result.push('\n'),
                        b'r' => result.push('\r'),
                        b't' => result.push('\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xd800..0xdc00).contains(&code) && self.bytes[self.position..].starts_with(b"\\u") {
                                // a pair of surrogates makes one character
                                let before = self.position;
                                self.position += 2;
                                let low = self.hex4()?;
                                if (0xdc00..0xe000).contains(&low) {
                                    code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
                                } else {
                                    self.position = before;
                                }
                            }
                            // half a pair can't be kept in a text of Rust
                            result.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(self.error("unknown escape")),
                    }
                }
                Some(_) => return Err(self.error("a control character in a text")),
                None => return Err(self.error("a text without an end")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.position += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected , or ]")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.position += 1;
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a name"));
            }
            let name = self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(self.error("expected :"));
            }
            self.position += 1;
            self.skip_whitespace();
            entries.push((name, self.value(depth + 1)?));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    return Ok(Json::Object(entries));
                }
                _ => return Err(self.error("expected , or }")),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Java
// ---------------------------------------------------------------------------------------------

/// `String.trim()` of Java: without the characters up to the space (U+0020) at both ends.
pub fn java_trim(text: &str) -> &str {
    text.trim_matches(|character: char| character <= ' ')
}

/// `Double.toString` of Java (of a JDK from 19 on, which writes the shortest digits).
pub fn java_double_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let magnitude = value.abs();
    if magnitude == 0.0 || (1e-3..1e7).contains(&magnitude) {
        // 0.001 to 9999999.99…: plain digits, with a fraction
        return format!("{value:?}");
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    if mantissa.contains('.') {
        format!("{mantissa}E{exponent}")
    } else {
        format!("{mantissa}.0E{exponent}")
    }
}

// the text of a number node of Jackson: a whole number in digits, any other like Double.toString
fn java_number_text(number: &JsonNumber) -> String {
    if number.is_integer() {
        match number.as_i64() {
            Some(value) => value.to_string(),
            // a BigInteger
            None => number.as_str().to_string(),
        }
    } else {
        java_double_to_string(number.as_f64())
    }
}

// `String.valueOf` of what Jackson reads into an `Object`: a text, a number, a boolean, a list or
// a map
fn java_value_of(json: &Json) -> String {
    match json {
        Json::Null => "null".to_string(),
        Json::Bool(value) => value.to_string(),
        Json::Number(number) => java_number_text(number),
        Json::String(text) => text.clone(),
        Json::Array(items) => format!("[{}]", items.iter().map(java_value_of).collect::<Vec<_>>().join(", ")),
        Json::Object(_) => format!(
            "{{{}}}",
            json.properties()
                .iter()
                .map(|(key, value)| format!("{key}={}", java_value_of(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

// ---------------------------------------------------------------------------------------------
// Reading the values like Jackson
// ---------------------------------------------------------------------------------------------

// where a value is in the JSON, for the errors
#[derive(Clone, Copy)]
struct At<'a> {
    parent: Option<&'a At<'a>>,
    segment: Segment<'a>,
}

#[derive(Clone, Copy)]
enum Segment<'a> {
    Root,
    Key(&'a str),
    Index(usize),
}

impl<'a> At<'a> {
    fn root() -> At<'static> {
        At {
            parent: None,
            segment: Segment::Root,
        }
    }

    fn key<'b>(&'b self, key: &'b str) -> At<'b>
    where
        'a: 'b,
    {
        At {
            parent: Some(self),
            segment: Segment::Key(key),
        }
    }

    fn index<'b>(&'b self, index: usize) -> At<'b>
    where
        'a: 'b,
    {
        At {
            parent: Some(self),
            segment: Segment::Index(index),
        }
    }

    fn path(&self) -> String {
        let mut path = self.parent.map(At::path).unwrap_or_default();
        match self.segment {
            Segment::Root => path.push('$'),
            Segment::Key(key) => {
                path.push('.');
                path.push_str(key);
            }
            Segment::Index(index) => path.push_str(&format!("[{index}]")),
        }
        path
    }

    fn error(&self, message: &str) -> String {
        format!("{}: {message}", self.path())
    }
}

fn kind(json: &Json) -> &'static str {
    match json {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(_) => "a number",
        Json::String(_) => "a text",
        Json::Array(_) => "an array",
        Json::Object(_) => "an object",
    }
}

// what was read: None for a value which is null (or read as null), which keeps the default
type Read<T> = Result<Option<T>, String>;

// a field takes what was read: a nullable one (an Option) becomes Some of it
trait Slot<T> {
    fn put(&mut self, value: T);
}

impl<T> Slot<T> for T {
    fn put(&mut self, value: T) {
        *self = value;
    }
}

impl<T> Slot<T> for Option<T> {
    fn put(&mut self, value: T) {
        *self = Some(value);
    }
}

// a class Jackson reads property by property
trait Bean: Default {
    // sets a property, false for one the class doesn't have
    fn set_property(&mut self, key: &str, value: &Json, at: &At) -> Result<bool, String>;
}

fn read_bean<T: Bean>(json: &Json, at: &At) -> Result<T, String> {
    match json {
        Json::Object(entries) => {
            let mut bean = T::default();
            for (key, value) in entries {
                bean.set_property(key, value, at)?;
            }
            Ok(bean)
        }
        other => Err(at.error(&format!("expected an object, not {}", kind(other)))),
    }
}

fn bean<T: Bean>(json: &Json, at: &At) -> Read<T> {
    match json {
        Json::Null => Ok(None),
        other => read_bean(other, at).map(Some),
    }
}

// a String: a number is taken as it is written
fn text(json: &Json, at: &At) -> Read<String> {
    match json {
        Json::Null => Ok(None),
        Json::String(text) => Ok(Some(text.clone())),
        Json::Number(number) => Ok(Some(number.as_str().to_string())),
        Json::Bool(value) => Ok(Some(value.to_string())),
        other => Err(at.error(&format!("expected a text, not {}", kind(other)))),
    }
}

// a text with a default its setter falls back to when it is empty
fn text_or(default: &'static str) -> impl Fn(&Json, &At) -> Read<String> {
    move |json, at| Ok(text(json, at)?.map(|text| if text.is_empty() { default.to_string() } else { text }))
}

// a number written as a text, like Double.parseDouble reads it after Jackson (the hexadecimal
// ones aside)
fn parse_java_double(text: &str) -> Option<f64> {
    match text {
        "NaN" | "+NaN" | "-NaN" => return Some(f64::NAN),
        "Infinity" | "+Infinity" | "INF" => return Some(f64::INFINITY),
        "-Infinity" | "-INF" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let number = text.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(text);
    let unsigned = number.strip_prefix(['+', '-']).unwrap_or(number);
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(index) => (&unsigned[..index], Some(&unsigned[index + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || !digits(fraction) || (whole.is_empty() && fraction.is_empty()) {
        return None;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if exponent.is_empty() || !digits(exponent) {
            return None;
        }
    }
    number.parse::<f64>().ok()
}

fn double_of(json: &Json, at: &At, primitive: bool) -> Read<f64> {
    match json {
        Json::Null => Ok(None),
        Json::Number(number) => Ok(Some(number.as_f64())),
        Json::String(text) => {
            let trimmed = java_trim(text);
            if trimmed.is_empty() {
                // read as null, which a primitive can't be
                return if primitive {
                    Err(at.error("an empty text is not a number"))
                } else {
                    Ok(None)
                };
            }
            parse_java_double(trimmed)
                .map(Some)
                .ok_or_else(|| at.error(&format!("\"{text}\" is not a number")))
        }
        other => Err(at.error(&format!("expected a number, not {}", kind(other)))),
    }
}

// a double
fn num(json: &Json, at: &At) -> Read<f64> {
    double_of(json, at, true)
}

// a Double
fn num_w(json: &Json, at: &At) -> Read<f64> {
    double_of(json, at, false)
}

fn long_of(json: &Json, at: &At, primitive: bool, min: i64, max: i64) -> Read<i64> {
    let out_of_range = || at.error("a number out of range");
    match json {
        Json::Null => Ok(None),
        Json::Number(number) if number.is_integer() => match number.as_i64() {
            Some(value) if value >= min && value <= max => Ok(Some(value)),
            _ => Err(out_of_range()),
        },
        Json::Number(number) => {
            // a number with a fraction is cut to the whole one
            let value = number.as_f64();
            if value < min as f64 || value > max as f64 {
                return Err(out_of_range());
            }
            Ok(Some(value as i64))
        }
        Json::String(text) => {
            let trimmed = java_trim(text);
            if trimmed.is_empty() {
                return if primitive {
                    Err(at.error("an empty text is not a number"))
                } else {
                    Ok(None)
                };
            }
            let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(at.error(&format!("\"{text}\" is not a whole number")));
            }
            match trimmed.parse::<i64>() {
                Ok(value) if value >= min && value <= max => Ok(Some(value)),
                _ => Err(out_of_range()),
            }
        }
        other => Err(at.error(&format!("expected a whole number, not {}", kind(other)))),
    }
}

// an int
fn int(json: &Json, at: &At) -> Read<i32> {
    Ok(long_of(json, at, true, i32::MIN as i64, i32::MAX as i64)?.map(|value| value as i32))
}

// an Integer
fn int_w(json: &Json, at: &At) -> Read<i32> {
    Ok(long_of(json, at, false, i32::MIN as i64, i32::MAX as i64)?.map(|value| value as i32))
}

// a long
fn long(json: &Json, at: &At) -> Read<i64> {
    long_of(json, at, true, i64::MIN, i64::MAX)
}

fn boolean_of(json: &Json, at: &At, primitive: bool) -> Read<bool> {
    match json {
        Json::Null => Ok(None),
        Json::Bool(value) => Ok(Some(*value)),
        // 0 is false, any other whole number true
        Json::Number(number) if number.is_integer() => {
            Ok(Some(number.as_str().bytes().any(|byte| (b'1'..=b'9').contains(&byte))))
        }
        Json::String(text) => match java_trim(text) {
            "true" | "True" | "TRUE" => Ok(Some(true)),
            "false" | "False" | "FALSE" => Ok(Some(false)),
            "" if !primitive => Ok(None),
            _ => Err(at.error(&format!("\"{text}\" is not a boolean"))),
        },
        other => Err(at.error(&format!("expected a boolean, not {}", kind(other)))),
    }
}

// a boolean
fn boolean(json: &Json, at: &At) -> Read<bool> {
    boolean_of(json, at, true)
}

// a Boolean
fn boolean_w(json: &Json, at: &At) -> Read<bool> {
    boolean_of(json, at, false)
}

// a list (or an array) which holds no null
fn list<T>(element: impl Fn(&Json, &At) -> Read<T>) -> impl Fn(&Json, &At) -> Read<Vec<T>> {
    move |json, at| match json {
        Json::Null => Ok(None),
        Json::Array(items) => {
            let mut result = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let at = at.index(index);
                match element(item, &at)? {
                    Some(value) => result.push(value),
                    None => {
                        return Err(at.error("null is not read in this list (Java would keep it and fail on it later)"))
                    }
                }
            }
            Ok(Some(result))
        }
        other => Err(at.error(&format!("expected an array, not {}", kind(other)))),
    }
}

// a list which may hold null
fn list_opt<T>(element: impl Fn(&Json, &At) -> Read<T>) -> impl Fn(&Json, &At) -> Read<Vec<Option<T>>> {
    move |json, at| match json {
        Json::Null => Ok(None),
        Json::Array(items) => {
            let mut result = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                result.push(element(item, &at.index(index))?);
            }
            Ok(Some(result))
        }
        other => Err(at.error(&format!("expected an array, not {}", kind(other)))),
    }
}

// a map, read into a LinkedHashMap by Jackson: in the order of the JSON, a name written twice at
// its first place with its last value
fn ordered_map<T>(element: impl Fn(&Json, &At) -> Read<T>) -> impl Fn(&Json, &At) -> Read<OrderedMap<T>> {
    move |json, at| match json {
        Json::Null => Ok(None),
        Json::Object(_) => {
            let mut entries = Vec::new();
            for (key, value) in json.properties() {
                let at = at.key(key);
                match element(value, &at)? {
                    Some(value) => entries.push((key.to_string(), value)),
                    None => {
                        return Err(at.error("null is not read in this map (Java would keep it and fail on it later)"))
                    }
                }
            }
            Ok(Some(OrderedMap { entries }))
        }
        other => Err(at.error(&format!("expected an object, not {}", kind(other)))),
    }
}

// an enum Jackson reads by name, or by its place in the declaration (a whole number, also written
// as a text). A name nobody knows is read as null.
trait JsonEnum: Sized + Copy + 'static {
    const VALUES: &'static [Self];
    fn name_of(self) -> &'static str;
}

fn enumeration<E: JsonEnum>(json: &Json, at: &At) -> Read<E> {
    let by_name = |name: &str| E::VALUES.iter().copied().find(|value| value.name_of() == name);
    match json {
        Json::Null => Ok(None),
        Json::String(text) => {
            if let Some(value) = by_name(text) {
                return Ok(Some(value));
            }
            let trimmed = java_trim(text);
            if let Some(value) = by_name(trimmed) {
                return Ok(Some(value));
            }
            // the place in the enum, but not written like "01"
            if trimmed.starts_with(|character: char| character.is_ascii_digit())
                && (trimmed.len() == 1 || !trimmed.starts_with('0'))
            {
                return Ok(trimmed
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| E::VALUES.get(index).copied()));
            }
            Ok(None)
        }
        Json::Number(number) if number.is_integer() => match number.as_i64() {
            Some(index) if index >= i32::MIN as i64 && index <= i32::MAX as i64 => Ok(usize::try_from(index)
                .ok()
                .and_then(|index| E::VALUES.get(index).copied())),
            _ => Err(at.error("a number out of range")),
        },
        other => Err(at.error(&format!("expected a name, not {}", kind(other)))),
    }
}

/// A map which keeps the order its entries are written in, like the `LinkedHashMap` Jackson reads
/// the channels and the wheels of a profile into.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedMap<T> {
    pub entries: Vec<(String, T)>,
}

impl<T> Default for OrderedMap<T> {
    fn default() -> Self {
        OrderedMap { entries: Vec::new() }
    }
}

impl<T> OrderedMap<T> {
    pub fn get(&self, key: &str) -> Option<&T> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.entries.iter().map(|(key, value)| (key.as_str(), value))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(key, _)| key.as_str())
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|(_, value)| value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// A class of the model: its fields with the name they have in the JSON, their default and how
// they are read. A class which extends another one holds it in a field and hands the properties
// it doesn't have itself on to it.
macro_rules! bean {
    (
        $(#[$meta:meta])*
        pub struct $name:ident $(extends $parent:ident: $parent_type:ty)? {
            $(
                $(#[$field_meta:meta])*
                $json:literal => $field:ident: $type:ty = $default:expr, $reader:expr;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name {
            $(pub $parent: $parent_type,)?
            $(
                $(#[$field_meta])*
                pub $field: $type,
            )*
        }

        impl Default for $name {
            fn default() -> Self {
                $name {
                    $($parent: Default::default(),)?
                    $($field: $default,)*
                }
            }
        }

        impl Bean for $name {
            fn set_property(&mut self, key: &str, value: &Json, at: &At) -> Result<bool, String> {
                let this = self;
                match key {
                    $(
                        $json => {
                            let reader = $reader;
                            if let Some(read) = reader(value, &at.key($json))? {
                                Slot::put(&mut this.$field, read);
                            }
                            Ok(true)
                        }
                    )*
                    _ => bean!(@inherited this, key, value, at $(, $parent)?),
                }
            }
        }

        impl $name {
            /// Reads it like Jackson reads the class.
            pub fn from_json(json: &Json) -> Result<Self, String> {
                read_bean(json, &At::root())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let json = <Json as serde::Deserialize>::deserialize(deserializer)?;
                Self::from_json(&json).map_err(serde::de::Error::custom)
            }
        }
    };
    (@inherited $this:ident, $key:ident, $value:ident, $at:ident) => {{
        let _ = (&$this, $key, $value, $at);
        Ok(false)
    }};
    (@inherited $this:ident, $key:ident, $value:ident, $at:ident, $parent:ident) => {
        Bean::set_property(&mut $this.$parent, $key, $value, $at)
    };
}

// An enum of the model, with the names it has in the JSON, in the order of the Java enum. Its
// numbers count from the passed first one in this order.
macro_rules! json_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident from $first:literal {
            $($variant:ident = $json:literal),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($variant),*
        }

        impl $name {
            /// All of them, in the order of the Java enum.
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            /// The name in the JSON.
            pub fn json_name(self) -> &'static str {
                match self {
                    $($name::$variant => $json),*
                }
            }

            pub fn from_json_name(name: &str) -> Option<$name> {
                match name {
                    $($json => Some($name::$variant),)*
                    _ => None,
                }
            }

            /// The stable number: the place in the Java enum, counted from the first number.
            pub fn as_u8(self) -> u8 {
                self as u8 + $first
            }

            pub fn from_u8(number: u8) -> Option<$name> {
                number.checked_sub($first).and_then(|index| Self::ALL.get(index as usize).copied())
            }
        }

        impl JsonEnum for $name {
            const VALUES: &'static [$name] = $name::ALL;

            fn name_of(self) -> &'static str {
                self.json_name()
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// The project
// ---------------------------------------------------------------------------------------------

bean! {
    /// A Rocket Show Designer project (`Project`).
    pub struct Project {
        /// The project-file version (0 where it is not written).
        "version" => version: i32 = 0, int;
        "uuid" => uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        "masterDimmerValue" => master_dimmer_value: f64 = 1.0, num;
        "selectedPresetUuid" => selected_preset_uuid: Option<String> = None, text;
        "selectedStepUuid" => selected_step_uuid: Option<String> = None, text;
        "stepPreviewRunning" => step_preview_running: bool = false, boolean;
        "stepPreviewStartMillis" => step_preview_start_millis: f64 = 0.0, num;
        "selectedSceneUuids" => selected_scene_uuids: Vec<String> = Vec::new(), list(text);
        "identifyFixtureUuids" => identify_fixture_uuids: Vec<String> = Vec::new(), list(text);
        "resetFixtureUuids" => reset_fixture_uuids: Vec<String> = Vec::new(), list(text);
        "lampOnFixtureUuids" => lamp_on_fixture_uuids: Vec<String> = Vec::new(), list(text);
        "dmxOverrides" => dmx_overrides: Vec<DmxOverride> = Vec::new(), list(bean::<DmxOverride>);
        "previewHolds" => preview_holds: Vec<CompositionHold> = Vec::new(), list(bean::<CompositionHold>);
        "previewHolding" => preview_holding: bool = false, boolean;
        /// The stage the fixtures are spread over, in cm.
        "stageWidthCm" => stage_width_cm: f64 = 600.0, num;
        "stageHeightCm" => stage_height_cm: f64 = 350.0, num;
        "stageDepthCm" => stage_depth_cm: f64 = 600.0, num;
        "stageFloorHeightCm" => stage_floor_height_cm: f64 = 30.0, num;
        "selectedCompositionUuid" => selected_composition_uuid: Option<String> = None, text;
        "previewPreset" => preview_preset: bool = false, boolean;
        "colorDimmersAtFull" => color_dimmers_at_full: bool = true, boolean;
        "compositions" => compositions: Option<Vec<Composition>> = None, list(bean::<Composition>);
        "fixtureProfiles" => fixture_profiles: Option<Vec<FixtureProfile>> = None, list(bean::<FixtureProfile>);
        "fixtures" => fixtures: Option<Vec<Fixture>> = None, list(bean::<Fixture>);
        /// The fixtures and pixel keys in the order they are selectable (and chased) in.
        "presetFixtures" => preset_fixtures: Option<Vec<PresetFixture>> = None, list(bean::<PresetFixture>);
        "scenes" => scenes: Option<Vec<Scene>> = None, list(bean::<Scene>);
        "sceneFolders" => scene_folders: Option<Vec<SceneFolder>> = None, list(bean::<SceneFolder>);
        "presets" => presets: Option<Vec<Preset>> = None, list(bean::<Preset>);
    }
}

bean! {
    /// A fixture of the project (`Fixture`).
    pub struct Fixture {
        "uuid" => uuid: Option<String> = None, text;
        "profileUuid" => profile_uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        "dmxUniverseUuid" => dmx_universe_uuid: String = String::new(), text;
        "dmxFirstChannel" => dmx_first_channel: i32 = 0, int;
        "modeShortName" => mode_short_name: Option<String> = None, text;
        "positioning" => positioning: String = "topFront".to_string(), text;
        "positionX" => position_x: f64 = 0.0, num;
        "positionY" => position_y: f64 = 0.0, num;
        "positionZ" => position_z: f64 = 0.0, num;
        "rotationX" => rotation_x: f64 = 0.0, num;
        "rotationY" => rotation_y: f64 = 0.0, num;
        "rotationZ" => rotation_z: f64 = 0.0, num;
    }
}

bean! {
    /// A fixture (or one of its pixels) referenced by a preset (`PresetFixture`).
    pub struct PresetFixture {
        "fixtureUuid" => fixture_uuid: Option<String> = None, text;
        "pixelKey" => pixel_key: Option<String> = None, text;
    }
}

bean! {
    /// A preset (`Preset`).
    pub struct Preset {
        "uuid" => uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        /// Obsolete: the fixtures of a project before version 2.
        "fixtureUuids" => fixture_uuids: Option<Vec<String>> = None, list(text);
        "fixtures" => fixtures: Vec<PresetFixture> = Vec::new(), list(bean::<PresetFixture>);
        "useGlobalFixtureOrder" => use_global_fixture_order: bool = true, boolean;
        /// Obsolete: the values of the first step, for a player which doesn't know steps.
        "fixtureChannelValues" => fixture_channel_values: Option<Vec<FixtureChannelValue>> = None, list(bean::<FixtureChannelValue>);
        "fixtureCapabilityValues" => fixture_capability_values: Option<Vec<FixtureCapabilityValue>> = None, list(bean::<FixtureCapabilityValue>);
        "steps" => steps: Vec<PresetStep> = Vec::new(), list(bean::<PresetStep>);
        "stepsLoop" => steps_loop: bool = false, boolean;
        "stepsLoopMillis" => steps_loop_millis: Option<f64> = None, num_w;
        "stepsPhasingMode" => steps_phasing_mode: String = "millis".to_string(), text;
        "stepsPhasingMillis" => steps_phasing_millis: f64 = 0.0, num;
        "stepsPhasingCycles" => steps_phasing_cycles: f64 = 1.0, num;
        "stepsPhasingGroupSize" => steps_phasing_group_size: f64 = 1.0, num;
        "stepsLengthMode" => steps_length_mode: String = "millis".to_string(), text;
        "stepsBeatsPerMinute" => steps_beats_per_minute: f64 = 120.0, num;
        "stepsOrder" => steps_order: String = "forward".to_string(), text;
        "stepsSeed" => steps_seed: f64 = 1.0, num;
        "effects" => effects: Vec<Effect> = Vec::new(), list(effect);
        "mirrorPan" => mirror_pan: bool = false, boolean;
        "mirrorTilt" => mirror_tilt: bool = false, boolean;
        /// "preset", also for an empty one (`setFanOrder`).
        "fanOrder" => fan_order: String = "preset".to_string(), text_or("preset");
        "patternGroupSize" => pattern_group_size: f64 = 1.0, num;
        "patternWings" => pattern_wings: f64 = 1.0, num;
        "patternEvery" => pattern_every: f64 = 0.0, num;
        "emittersFromColor" => emitters_from_color: bool = false, boolean;
        /// The offset to the start of the scene (None: the start or the end of the scene itself).
        "startMillis" => start_millis: Option<f64> = None, num_w;
        "endMillis" => end_millis: Option<f64> = None, num_w;
        "fadeInMillis" => fade_in_millis: f64 = 0.0, num;
        "fadeOutMillis" => fade_out_millis: f64 = 0.0, num;
        "fadeInPre" => fade_in_pre: bool = false, boolean;
        "fadeOutPost" => fade_out_post: bool = false, boolean;
        /// The name of a transition curve (see `TransitionCurve` of the engine).
        "fadeInCurve" => fade_in_curve: String = "linear".to_string(), text;
        "fadeOutCurve" => fade_out_curve: String = "linear".to_string(), text;
    }
}

impl Preset {
    /// Whether the preset mirrors what it puts on the passed capability (`mirrorsCapability`).
    pub fn mirrors_capability(&self, capability_type: Option<FixtureCapabilityType>) -> bool {
        (capability_type == Some(FixtureCapabilityType::Pan) && self.mirror_pan)
            || (capability_type == Some(FixtureCapabilityType::Tilt) && self.mirror_tilt)
    }

    /// The position the preset points its fixtures at on the passed capability
    /// (`getMirroredValuePercentage`).
    pub fn get_mirrored_value_percentage(
        &self,
        capability_type: Option<FixtureCapabilityType>,
        value_percentage: f64,
    ) -> f64 {
        if self.mirrors_capability(capability_type) {
            1.0 - value_percentage
        } else {
            value_percentage
        }
    }
}

bean! {
    /// One state of a preset (`PresetStep`).
    pub struct PresetStep {
        "uuid" => uuid: Option<String> = None, text;
        "startMillis" => start_millis: f64 = 0.0, num;
        /// None: the whole time the step lasts, 0: a jump.
        "transitionMillis" => transition_millis: Option<f64> = None, num_w;
        "transitionCurve" => transition_curve: String = "linear".to_string(), text;
        "fixtureChannelValues" => fixture_channel_values: Vec<FixtureChannelValue> = Vec::new(), list(bean::<FixtureChannelValue>);
        "fixtureCapabilityValues" => fixture_capability_values: Vec<FixtureCapabilityValue> = Vec::new(), list(bean::<FixtureCapabilityValue>);
        "effectAmounts" => effect_amounts: Vec<PresetStepEffectAmount> = Vec::new(), list(bean::<PresetStepEffectAmount>);
    }
}

impl PresetStep {
    /// How much of the passed effect the step lets through: 1 for one it says nothing about
    /// (`getEffectAmount`).
    pub fn get_effect_amount(&self, effect_uuid: Option<&str>) -> f64 {
        let Some(effect_uuid) = effect_uuid else {
            return 1.0;
        };
        self.effect_amounts
            .iter()
            .find(|amount| amount.effect_uuid.as_deref() == Some(effect_uuid))
            .map_or(1.0, |amount| amount.amount)
    }
}

bean! {
    /// How much of an effect a step lets through (`PresetStepEffectAmount`).
    pub struct PresetStepEffectAmount {
        "effectUuid" => effect_uuid: Option<String> = None, text;
        "amount" => amount: f64 = 1.0, num;
    }
}

bean! {
    /// A value of a channel (`FixtureChannelValue`).
    pub struct FixtureChannelValue {
        "channelName" => channel_name: Option<String> = None, text;
        "profileUuid" => profile_uuid: Option<String> = None, text;
        "value" => value: Option<f64> = None, num_w;
        "deactivated" => deactivated: Option<bool> = None, boolean_w;
    }
}

// a fan without any stops is no fan (setFan)
fn fan(json: &Json, at: &At) -> Read<Option<Vec<FanStop>>> {
    Ok(list(bean::<FanStop>)(json, at)?.map(|stops| if stops.is_empty() { None } else { Some(stops) }))
}

bean! {
    /// A value of a capability (`FixtureCapabilityValue`). What only lives while a preset travels
    /// between two steps (`transitionFrom`, `transitionPosition`) is not part of the JSON.
    pub struct FixtureCapabilityValue {
        "type" => type_: Option<FixtureCapabilityType> = None, enumeration::<FixtureCapabilityType>;
        "profileUuid" => profile_uuid: Option<String> = None, text;
        "valuePercentage" => value_percentage: Option<f64> = None, num_w;
        "color" => color: Option<FixtureCapabilityColor> = None, enumeration::<FixtureCapabilityColor>;
        "slotNumber" => slot_number: Option<f64> = None, num_w;
        "wheel" => wheel: Option<String> = None, text;
        "valueDegrees" => value_degrees: Option<f64> = None, num_w;
        "spreadDegrees" => spread_degrees: Option<f64> = None, num_w;
        "fanShape" => fan_shape: Option<String> = None, text;
        /// None also for a fan written without any stops.
        "fan" => fan: Option<Vec<FanStop>> = None, fan;
        "fanBetween" => fan_between: Option<String> = None, text;
        "aimX" => aim_x: Option<f64> = None, num_w;
        "aimY" => aim_y: Option<f64> = None, num_w;
        "aimZ" => aim_z: Option<f64> = None, num_w;
        "blade" => blade: Option<String> = None, text;
        "shutterEffect" => shutter_effect: Option<String> = None, text;
        "programNumber" => program_number: Option<f64> = None, num_w;
        "soundControlled" => sound_controlled: Option<bool> = None, boolean_w;
        "maintenanceAction" => maintenance_action: Option<String> = None, text;
        "deactivated" => deactivated: Option<bool> = None, boolean_w;
    }
}

bean! {
    /// A value of a fan at its place along it (`FanStop`).
    pub struct FanStop {
        "position" => position: f64 = 0.0, num_w;
        "valuePercentage" => value_percentage: f64 = 0.0, num_w;
    }
}

/// The capability values a step plays: all but the ones turned off (`PlayedValues.ofCapabilities`).
pub fn played_capability_values(values: &[FixtureCapabilityValue]) -> Vec<&FixtureCapabilityValue> {
    values.iter().filter(|value| value.deactivated != Some(true)).collect()
}

/// The channel values a step plays: all but the ones turned off (`PlayedValues.ofChannels`).
pub fn played_channel_values(values: &[FixtureChannelValue]) -> Vec<&FixtureChannelValue> {
    values.iter().filter(|value| value.deactivated != Some(true)).collect()
}

// ---------------------------------------------------------------------------------------------
// The effects
// ---------------------------------------------------------------------------------------------

json_enum! {
    /// `Effect.EffectChannel`.
    pub enum EffectChannel from 0 {
        ColorRed = "colorRed",
        ColorGreen = "colorGreen",
        ColorBlue = "colorBlue",
        Dimmer = "dimmer",
        Pan = "pan",
        Tilt = "tilt",
    }
}

/// An effect of a preset (`Effect`), by its `"type"`: `curve`, `pan-tilt`, `pixel`, or any other
/// (also none), which lights nothing.
// (the effects are not boxed: there are few of them, and they are matched on as they are)
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Curve(EffectCurve),
    PanTilt(EffectPanTilt),
    Pixel(EffectPixel),
    Unknown(EffectUnknown),
}

impl Effect {
    /// Reads it like Jackson reads an effect.
    pub fn from_json(json: &Json) -> Result<Effect, String> {
        let at = At::root();
        effect(json, &at)?.ok_or_else(|| at.error("expected an effect, not null"))
    }

    /// What every effect has.
    pub fn base(&self) -> &EffectBase {
        match self {
            Effect::Curve(curve) => &curve.base,
            Effect::PanTilt(pan_tilt) => &pan_tilt.curve.base,
            Effect::Pixel(pixel) => &pixel.curve.base,
            Effect::Unknown(unknown) => &unknown.base,
        }
    }

    pub fn uuid(&self) -> Option<&str> {
        self.base().uuid.as_deref()
    }

    /// The curve (and with it the timing) of the effect, which all but an unknown one have.
    pub fn curve(&self) -> Option<&EffectCurve> {
        match self {
            Effect::Curve(curve) => Some(curve),
            Effect::PanTilt(pan_tilt) => Some(&pan_tilt.curve),
            Effect::Pixel(pixel) => Some(&pixel.curve),
            Effect::Unknown(_) => None,
        }
    }
}

impl<'de> serde::Deserialize<'de> for Effect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let json = <Json as serde::Deserialize>::deserialize(deserializer)?;
        Effect::from_json(&json).map_err(serde::de::Error::custom)
    }
}

// The class of an effect is named by the first "type" which is a text (a number and a boolean
// count as their text), wherever it is written. A name nobody knows, also none, is an unknown
// effect.
fn effect(json: &Json, at: &At) -> Read<Effect> {
    let entries = match json {
        Json::Null => return Ok(None),
        Json::Object(entries) => entries,
        other => return Err(at.error(&format!("expected an effect, not {}", kind(other)))),
    };
    let type_name = entries.iter().find_map(|(key, value)| match value {
        Json::String(text) if key == "type" => Some(text.clone()),
        Json::Number(number) if key == "type" => Some(number.as_str().to_string()),
        Json::Bool(value) if key == "type" => Some(value.to_string()),
        _ => None,
    });
    Ok(Some(match type_name.as_deref() {
        Some("curve") => Effect::Curve(read_bean(json, at)?),
        Some("pan-tilt") => Effect::PanTilt(read_bean(json, at)?),
        Some("pixel") => Effect::Pixel(read_bean(json, at)?),
        _ => Effect::Unknown(read_bean(json, at)?),
    }))
}

bean! {
    /// What every effect has (the fields of `Effect`).
    pub struct EffectBase {
        "uuid" => uuid: Option<String> = None, text;
        /// Not played by the engine. A channel nobody knows is None.
        "effectChannels" => effect_channels: Option<Vec<Option<EffectChannel>>> = None, list_opt(enumeration::<EffectChannel>);
        "visible" => visible: bool = true, boolean;
    }
}

bean! {
    /// An effect of a type the engine does not know (`EffectUnknown`).
    pub struct EffectUnknown extends base: EffectBase {
        /// The type the designer gave it.
        "type" => type_: Option<String> = None, text;
    }
}

bean! {
    /// A curve on capabilities and channels (`EffectCurve`), and the timing of every other effect.
    pub struct EffectCurve extends base: EffectBase {
        "curveType" => curve_type: String = "sine".to_string(), text;
        "capabilities" => capabilities: Vec<FixtureCapability> = Vec::new(), list(bean::<FixtureCapability>);
        "channels" => channels: Vec<EffectCurveProfileChannels> = Vec::new(), list(bean::<EffectCurveProfileChannels>);
        "lengthMode" => length_mode: String = "millis".to_string(), text;
        "lengthMillis" => length_millis: f64 = 2500.0, num;
        "lengthBeats" => length_beats: f64 = 4.0, num;
        "phaseMillis" => phase_millis: f64 = 0.0, num;
        "phaseBeats" => phase_beats: f64 = 0.0, num;
        "amplitude" => amplitude: f64 = 1.0, num;
        "position" => position: f64 = 0.5, num;
        "dutyCycle" => duty_cycle: f64 = 0.5, num;
        "runMode" => run_mode: String = "infinite".to_string(), text;
        "runCycles" => run_cycles: f64 = 1.0, num;
        "runDurationMillis" => run_duration_millis: f64 = 5000.0, num;
        "endMode" => end_mode: String = "hold".to_string(), text;
        "phasingMode" => phasing_mode: String = "millis".to_string(), text;
        "phasingMillis" => phasing_millis: f64 = 0.0, num;
        "phasingBeats" => phasing_beats: f64 = 0.0, num;
        "phasingCycles" => phasing_cycles: f64 = 1.0, num;
        "phasingGroupSize" => phasing_group_size: f64 = 1.0, num;
        /// "none", also for an empty one (`setFanMode`).
        "fanMode" => fan_mode: String = "none".to_string(), text_or("none");
        "panTiltInDegrees" => pan_tilt_in_degrees: bool = false, boolean_w;
    }
}

bean! {
    /// A movement of the moving heads (`EffectPanTilt`), on the timing of a curve.
    pub struct EffectPanTilt extends curve: EffectCurve {
        /// "circle", also for an empty one.
        "shape" => shape: String = "circle".to_string(), text_or("circle");
        "sizePanDegrees" => size_pan_degrees: f64 = 60.0, num_w;
        "sizeTiltDegrees" => size_tilt_degrees: f64 = 30.0, num_w;
    }
}

bean! {
    /// A pixel effect (`EffectPixel`), on the timing of a curve. A text which is empty keeps its
    /// default, like the setters of the Java class.
    pub struct EffectPixel extends curve: EffectCurve {
        "pattern" => pattern: String = "chase".to_string(), text_or("chase");
        "size" => size: f64 = 0.25, num;
        "softness" => softness: f64 = 0.3, num;
        "direction" => direction: String = "right".to_string(), text_or("right");
        "shape" => shape: String = "circle".to_string(), text_or("circle");
        "bounce" => bounce: bool = false, boolean;
        "seed" => seed: f64 = 1.0, num;
        "mirror" => mirror: bool = false, boolean;
        /// The palette of the color of the preset (`ref` "preset") where none is given.
        "palette" => palette: PixelPalette = PixelPalette::preset(), bean::<PixelPalette>;
        "spread" => spread: String = "fixture".to_string(), text_or("fixture");
        "view" => view: String = "front".to_string(), text_or("front");
        "whiteLed" => white_led: bool = false, boolean_w;
        "blend" => blend: String = "replace".to_string(), text_or("replace");
        "still" => still: bool = false, boolean_w;
        "bitmap" => bitmap: Option<PixelBitmap> = None, bean::<PixelBitmap>;
        "scroll" => scroll: String = "auto".to_string(), text_or("auto");
        "align" => align: String = "center".to_string(), text_or("center");
        "offsetY" => offset_y: f64 = 0.0, num_w;
        "fit" => fit: String = "contain".to_string(), text_or("contain");
        "textColors" => text_colors: String = "along".to_string(), text_or("along");
        "imageColors" => image_colors: String = "own".to_string(), text_or("own");
        "audioSource" => audio_source: String = "off".to_string(), text_or("off");
        "audioBand" => audio_band: String = "full".to_string(), text_or("full");
        "audioGainDb" => audio_gain_db: f64 = 0.0, num_w;
        "audioFloorDb" => audio_floor_db: f64 = -60.0, num_w;
        "audioAttackMillis" => audio_attack_millis: f64 = 20.0, num_w;
        "audioReleaseMillis" => audio_release_millis: f64 = 300.0, num_w;
        "audioFollows" => audio_follows: String = "brightness".to_string(), text_or("brightness");
        "audioBuses" => audio_buses: Vec<String> = Vec::new(), list(text);
    }
}

bean! {
    /// The channels of a profile a curve runs on (`EffectCurveProfileChannels`).
    pub struct EffectCurveProfileChannels {
        "profileUuid" => profile_uuid: Option<String> = None, text;
        "channels" => channels: Vec<String> = Vec::new(), list(text);
    }
}

bean! {
    /// The palette of a pixel effect (`PixelPalette`).
    pub struct PixelPalette {
        "stops" => stops: Vec<PixelPaletteStop> = Vec::new(), list(bean::<PixelPaletteStop>);
        "between" => between: String = "smooth".to_string(), text;
        "repeat" => repeat: String = "mirror".to_string(), text;
        /// Which palette of the designer it was taken from, or "preset" (`ref` in the JSON).
        "ref" => ref_: Option<String> = None, text;
    }
}

impl PixelPalette {
    /// The palette a pixel effect has where none is given: the shades of the color of the preset.
    pub fn preset() -> PixelPalette {
        PixelPalette {
            ref_: Some("preset".to_string()),
            ..PixelPalette::default()
        }
    }
}

bean! {
    /// A color of a palette at its place (`PixelPaletteStop`).
    pub struct PixelPaletteStop {
        "position" => position: f64 = 0.0, num;
        "color" => color: String = "#000000".to_string(), text;
    }
}

// 3 bytes for each pixel or else 1 (setChannels)
fn bitmap_channels(json: &Json, at: &At) -> Read<i32> {
    Ok(int_w(json, at)?.map(|channels| if channels == 3 { 3 } else { 1 }))
}

bean! {
    /// The picture of a text or an image of a pixel effect (`PixelBitmap`), as it is written: the
    /// frames are not decoded.
    pub struct PixelBitmap {
        "width" => width: i32 = 0, int_w;
        "height" => height: i32 = 0, int_w;
        /// 1 (a brightness) or 3 (red, green and blue): anything but 3 is 1.
        "channels" => channels: i32 = 1, bitmap_channels;
        /// The frames, each the bytes of its pixels in base 64. A frame which is null has none.
        "frames" => frames: Vec<Option<String>> = Vec::new(), list_opt(text);
        /// How long each frame shows, in ms (null, or none: 100 ms in the engine).
        "durations" => durations: Vec<Option<f64>> = Vec::new(), list_opt(num_w);
        "letterStarts" => letter_starts: Vec<Option<i32>> = Vec::new(), list_opt(int_w);
    }
}

// ---------------------------------------------------------------------------------------------
// Scenes and compositions
// ---------------------------------------------------------------------------------------------

bean! {
    /// A scene (`Scene`).
    pub struct Scene {
        "uuid" => uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        "folderUuid" => folder_uuid: Option<String> = None, text;
        "sortIndex" => sort_index: f64 = 0.0, num;
        "icon" => icon: Option<String> = None, text;
        "color" => color: Option<String> = None, text;
        "colorAuto" => color_auto: bool = true, boolean;
        "displayColor" => display_color: Option<String> = None, text;
        /// The presets, the topmost layer first.
        "presetUuids" => preset_uuids: Option<Vec<String>> = None, list(text);
        "dimmer" => dimmer: f64 = 1.0, num;
        "fadeInMillis" => fade_in_millis: f64 = 0.0, num;
        "fadeOutMillis" => fade_out_millis: f64 = 0.0, num;
        "fadeInPre" => fade_in_pre: bool = false, boolean;
        "fadeOutPost" => fade_out_post: bool = false, boolean;
        "fadeInCurve" => fade_in_curve: String = "linear".to_string(), text;
        "fadeOutCurve" => fade_out_curve: String = "linear".to_string(), text;
    }
}

bean! {
    /// A folder of the scenes (`SceneFolder`).
    pub struct SceneFolder {
        "uuid" => uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        "parentUuid" => parent_uuid: Option<String> = None, text;
        "sortIndex" => sort_index: f64 = 0.0, num;
    }
}

// the beats of a bar: 4 for none (setTimeSignatureUpper)
fn time_signature_upper(json: &Json, at: &At) -> Read<i32> {
    Ok(int(json, at)?.map(|upper| if upper > 0 { upper } else { 4 }))
}

bean! {
    /// A composition (`Composition`).
    pub struct Composition {
        "name" => name: Option<String> = None, text;
        "uuid" => uuid: Option<String> = None, text;
        "syncType" => sync_type: Option<String> = None, text;
        /// A whole number of milliseconds (a fraction is cut off).
        "durationMillis" => duration_millis: i64 = 0, long;
        "beatsPerMinute" => beats_per_minute: f64 = 120.0, num;
        /// 4 also for one which is not above 0.
        "timeSignatureUpper" => time_signature_upper: i32 = 4, time_signature_upper;
        "markers" => markers: Vec<CompositionMarker> = Vec::new(), list(bean::<CompositionMarker>);
        /// The lanes of the timeline, the topmost first.
        "tracks" => tracks: Vec<Track> = Vec::new(), list(bean::<Track>);
        "scenePlaybackRegions" => scene_playback_regions: Option<Vec<ScenePlaybackRegion>> = None, list(bean::<ScenePlaybackRegion>);
    }
}

bean! {
    /// A marker on the timeline (`CompositionMarker`): a "label" or a "hold".
    pub struct CompositionMarker {
        "uuid" => uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        "positionMillis" => position_millis: f64 = 0.0, num;
        "type" => type_: String = "label".to_string(), text;
    }
}

impl CompositionMarker {
    pub const TYPE_HOLD: &'static str = "hold";

    pub fn is_hold(&self) -> bool {
        self.type_ == Self::TYPE_HOLD
    }
}

bean! {
    /// A track of the timeline (`Track`).
    pub struct Track {
        "uuid" => uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        "hidden" => hidden: bool = false, boolean;
        "solo" => solo: bool = false, boolean;
    }
}

bean! {
    /// A scene on the timeline (`ScenePlaybackRegion`).
    pub struct ScenePlaybackRegion {
        "sceneUuid" => scene_uuid: Option<String> = None, text;
        "startMillis" => start_millis: f64 = 0.0, num;
        "endMillis" => end_millis: f64 = 0.0, num;
        "trackUuid" => track_uuid: Option<String> = None, text;
    }
}

bean! {
    /// A hold the timeline played through (`CompositionHold`).
    pub struct CompositionHold {
        "positionMillis" => position_millis: f64 = 0.0, num;
        "heldMillis" => held_millis: f64 = 0.0, num;
    }
}

bean! {
    /// A DMX channel held at a value (`DmxOverride`).
    pub struct DmxOverride {
        "universeUuid" => universe_uuid: Option<String> = None, text;
        /// Counted from 0.
        "dmxChannel" => dmx_channel: i32 = 0, int;
        "value" => value: i32 = 0, int;
    }
}

// ---------------------------------------------------------------------------------------------
// The fixture profiles
// ---------------------------------------------------------------------------------------------

json_enum! {
    /// `FixtureProfile.FixtureCategory`.
    pub enum FixtureCategory from 0 {
        Blinder = "Blinder",
        ColorChanger = "Color Changer",
        Dimmer = "Dimmer",
        Effect = "Effect",
        Fan = "Fan",
        Flower = "Flower",
        Hazer = "Hazer",
        Laser = "Laser",
        Matrix = "Matrix",
        MovingHead = "Moving Head",
        PixelBar = "Pixel Bar",
        Scanner = "Scanner",
        Smoke = "Smoke",
        Stand = "Stand",
        Strobe = "Strobe",
        Other = "Other",
        BarrelScanner = "Barrel Scanner",
    }
}

/// The channels of a profile by their name, in the order of the profile.
pub type FixtureProfileAvailableChannels = OrderedMap<FixtureChannel>;
/// The channels of the pixels of a profile by their name (e.g. "Red $pixelKey"), in the order of
/// the profile.
pub type FixtureProfileTemplateChannels = OrderedMap<FixtureChannel>;
/// The wheels of a profile by their name, in the order of the profile.
pub type FixtureProfileWheels = OrderedMap<FixtureWheel>;

bean! {
    /// A fixture profile (`FixtureProfile`).
    pub struct FixtureProfile {
        "uuid" => uuid: Option<String> = None, text;
        "name" => name: Option<String> = None, text;
        /// Not played by the engine. A category nobody knows is None.
        "categories" => categories: Option<Vec<Option<FixtureCategory>>> = None, list_opt(enumeration::<FixtureCategory>);
        "physical" => physical: Option<FixturePhysical> = None, bean::<FixturePhysical>;
        "matrix" => matrix: Option<FixtureMatrix> = None, bean::<FixtureMatrix>;
        "availableChannels" => available_channels: FixtureProfileAvailableChannels = OrderedMap::default(), ordered_map(bean::<FixtureChannel>);
        "templateChannels" => template_channels: FixtureProfileTemplateChannels = OrderedMap::default(), ordered_map(bean::<FixtureChannel>);
        "wheels" => wheels: FixtureProfileWheels = OrderedMap::default(), ordered_map(bean::<FixtureWheel>);
        "modes" => modes: Option<Vec<FixtureMode>> = None, list(bean::<FixtureMode>);
    }
}

bean! {
    /// A wheel of a profile (`FixtureWheel`).
    pub struct FixtureWheel {
        "slots" => slots: Vec<FixtureWheelSlot> = Vec::new(), list(bean::<FixtureWheelSlot>);
    }
}

json_enum! {
    /// `FixtureWheelSlot.FixtureWheelSlotType`.
    pub enum FixtureWheelSlotType from 0 {
        Open = "Open",
        Closed = "Closed",
        Color = "Color",
        Gobo = "Gobo",
        Prism = "Prism",
        Iris = "Iris",
        Frost = "Frost",
        AnimationGoboStart = "AnimationGoboStart",
        AnimationGoboEnd = "AnimationGoboEnd",
    }
}

bean! {
    /// A slot of a wheel (`FixtureWheelSlot`).
    pub struct FixtureWheelSlot {
        "type" => type_: Option<FixtureWheelSlotType> = None, enumeration::<FixtureWheelSlotType>;
        "name" => name: Option<String> = None, text;
        "colors" => colors: Option<Vec<String>> = None, list(text);
    }
}

// The channels of a mode (FixtureModeChannelListDeserializer): the name of a channel, or an object
// for the channels of the pixels. Anything else (null in the fixture library) is a channel
// nobody uses.
fn mode_channels(json: &Json, at: &At) -> Read<Vec<FixtureModeChannel>> {
    // like the loop over a JsonNode: the items of an array, the values of an object, or nothing
    let items: Vec<&Json> = match json {
        Json::Null => return Ok(None),
        Json::Array(items) => items.iter().collect(),
        Json::Object(_) => json.properties().into_iter().map(|(_, value)| value).collect(),
        _ => Vec::new(),
    };
    let mut channels = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        channels.push(match item {
            Json::String(name) => FixtureModeChannel {
                name: name.clone(),
                ..FixtureModeChannel::default()
            },
            Json::Object(_) => read_bean(item, &at.index(index))?,
            _ => FixtureModeChannel::default(),
        });
    }
    Ok(Some(channels))
}

bean! {
    /// A mode of a profile (`FixtureMode`).
    pub struct FixtureMode {
        "name" => name: Option<String> = None, text;
        "shortName" => short_name: Option<String> = None, text;
        "channels" => channels: Vec<FixtureModeChannel> = Vec::new(), mode_channels;
    }
}

// a list of texts, or a single text (StringOrListDeserializer)
fn string_or_list(json: &Json, at: &At) -> Read<Vec<String>> {
    match json {
        Json::Null => Ok(None),
        Json::Array(items) => {
            let mut result = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                result.push(item.as_text().map_err(|message| at.index(index).error(&message))?);
            }
            Ok(Some(result))
        }
        Json::String(text) => Ok(Some(vec![text.clone()])),
        _ => Ok(Some(Vec::new())),
    }
}

bean! {
    /// A channel of a mode (`FixtureModeChannel`): one of the profile by its name, the channels of
    /// the pixels (`repeatFor`), or none (no name and nothing repeated).
    pub struct FixtureModeChannel {
        "name" => name: String = String::new(), text;
        "insert" => insert: Option<String> = None, text;
        /// Pixel keys and groups, or one of "eachPixelABC", "eachPixelGroup", "eachPixelXYZ"…
        "repeatFor" => repeat_for: Vec<String> = Vec::new(), string_or_list;
        "channelOrder" => channel_order: Option<String> = None, text;
        "templateChannels" => template_channels: Vec<String> = Vec::new(), list(text);
    }
}

bean! {
    /// A channel of a profile (`FixtureChannel`).
    pub struct FixtureChannel {
        "fineChannelAliases" => fine_channel_aliases: Vec<String> = Vec::new(), list(text);
        /// A DMX value or a percentage ("50%"), as it is written (a number in its digits).
        "defaultValue" => default_value: Option<String> = None, text;
        "capability" => capability: Option<FixtureCapability> = None, bean::<FixtureCapability>;
        "capabilities" => capabilities: Option<Vec<FixtureCapability>> = None, list(bean::<FixtureCapability>);
        "dmxValueResolution" => dmx_value_resolution: Option<String> = None, text;
    }
}

impl FixtureChannel {
    /// How much the DMX values a profile gives the channel are multiplied by to be in the
    /// resolution of the channel with all of its fine channels (`getResolutionMultiplier`).
    pub fn get_resolution_multiplier(channel: Option<&FixtureChannel>) -> f64 {
        let Some(channel) = channel else {
            return 1.0;
        };
        let bytes = match channel.dmx_value_resolution.as_deref() {
            Some("8bit") => 1.0,
            Some("16bit") => 2.0,
            Some("24bit") => 3.0,
            _ => return 1.0,
        };
        let full_bytes = 1.0 + channel.fine_channel_aliases.len() as f64;
        if bytes < full_bytes {
            256f64.powi((full_bytes - bytes) as i32)
        } else {
            1.0
        }
    }

    /// The multiplier of this channel (see [`FixtureChannel::get_resolution_multiplier`]).
    pub fn resolution_multiplier(&self) -> f64 {
        Self::get_resolution_multiplier(Some(self))
    }

    /// A capability with its range in the full resolution of its channel, or the capability
    /// itself, if it is written in it (`getCapabilityInFullResolution`).
    pub fn get_capability_in_full_resolution(
        capability: &FixtureCapability,
        multiplier: f64,
    ) -> Cow<'_, FixtureCapability> {
        let range = match capability.dmx_range.as_deref() {
            Some(range) if multiplier != 1.0 && range.len() == 2 => range,
            _ => return Cow::Borrowed(capability),
        };
        let mut scaled = capability.clone();
        // cut to an int like the cast of Java (which stops at the ends of the ints as well)
        scaled.dmx_range = Some(vec![
            (range[0] as f64 * multiplier) as i32,
            (range[1] as f64 * multiplier + multiplier - 1.0) as i32,
        ]);
        Cow::Owned(scaled)
    }
}

json_enum! {
    /// `FixtureCapability.FixtureCapabilityType`, numbered from 0.
    pub enum FixtureCapabilityType from 0 {
        NoFunction = "NoFunction",
        ShutterStrobe = "ShutterStrobe",
        StrobeSpeed = "StrobeSpeed",
        StrobeDuration = "StrobeDuration",
        Intensity = "Intensity",
        ColorIntensity = "ColorIntensity",
        ColorPreset = "ColorPreset",
        ColorTemperature = "ColorTemperature",
        Pan = "Pan",
        PanContinuous = "PanContinuous",
        Tilt = "Tilt",
        TiltContinuous = "TiltContinuous",
        PanTiltSpeed = "PanTiltSpeed",
        WheelSlot = "WheelSlot",
        WheelShake = "WheelShake",
        WheelSlotRotation = "WheelSlotRotation",
        WheelRotation = "WheelRotation",
        EffectSpeed = "EffectSpeed",
        EffectDuration = "EffectDuration",
        EffectParameter = "EffectParameter",
        SoundSensitivity = "SoundSensitivity",
        Focus = "Focus",
        Zoom = "Zoom",
        Iris = "Iris",
        IrisEffect = "IrisEffect",
        Frost = "Frost",
        FrostEffect = "FrostEffect",
        Prism = "Prism",
        PrismRotation = "PrismRotation",
        BladeInsertion = "BladeInsertion",
        BladeRotation = "BladeRotation",
        BladeSystemRotation = "BladeSystemRotation",
        Fog = "Fog",
        FogOutput = "FogOutput",
        FogType = "FogType",
        BeamAngle = "BeamAngle",
        Rotation = "Rotation",
        Speed = "Speed",
        Time = "Time",
        Maintenance = "Maintenance",
        Generic = "Generic",
        Effect = "Effect",
        BeamPosition = "BeamPosition",
    }
}

json_enum! {
    /// `FixtureCapability.FixtureCapabilityColor`, numbered from 1 (0 is free for "none").
    pub enum FixtureCapabilityColor from 1 {
        Red = "Red",
        Green = "Green",
        Blue = "Blue",
        Cyan = "Cyan",
        Magenta = "Magenta",
        Yellow = "Yellow",
        Amber = "Amber",
        White = "White",
        WarmWhite = "Warm White",
        ColdWhite = "Cold White",
        UV = "UV",
        Lime = "Lime",
        Indigo = "Indigo",
    }
}

/// The wheel a capability names (`FixtureCapability.wheel`, an `Object` in Java).
#[derive(Debug, Clone, PartialEq)]
pub enum Wheel {
    /// The name of a wheel.
    Name(String),
    /// Several wheels (e.g. shaken together), each like `String.valueOf` gives it.
    Names(Vec<String>),
    /// Anything else (a number, a boolean, an object): the Java engine takes it for none.
    Other,
}

impl Wheel {
    /// The wheels named, or None where the engine falls back to the name of the channel.
    pub fn names(&self) -> Option<Vec<&str>> {
        match self {
            Wheel::Name(name) => Some(vec![name]),
            Wheel::Names(names) => Some(names.iter().map(String::as_str).collect()),
            Wheel::Other => None,
        }
    }
}

fn wheel(json: &Json, _at: &At) -> Read<Wheel> {
    Ok(match json {
        Json::Null => None,
        Json::String(name) => Some(Wheel::Name(name.clone())),
        Json::Array(names) => Some(Wheel::Names(names.iter().map(java_value_of).collect())),
        _ => Some(Wheel::Other),
    })
}

bean! {
    /// A capability of a channel (`FixtureCapability`). The properties of the fixture library are
    /// kept as it writes them (e.g. "10deg", "wide", "3200K").
    pub struct FixtureCapability {
        "type" => type_: Option<FixtureCapabilityType> = None, enumeration::<FixtureCapabilityType>;
        "color" => color: Option<FixtureCapabilityColor> = None, enumeration::<FixtureCapabilityColor>;
        "dmxRange" => dmx_range: Option<Vec<i32>> = None, list(int_w);
        "wheel" => wheel: Option<Wheel> = None, wheel;
        /// E.g. 2.5 for the split between the slots 2 and 3.
        "slotNumber" => slot_number: Option<f64> = None, num_w;
        "brightness" => brightness: Option<String> = None, text;
        "brightnessStart" => brightness_start: Option<String> = None, text;
        "brightnessEnd" => brightness_end: Option<String> = None, text;
        "shutterEffect" => shutter_effect: Option<String> = None, text;
        "randomTiming" => random_timing: Option<bool> = None, boolean_w;
        "soundControlled" => sound_controlled: Option<bool> = None, boolean_w;
        "speed" => speed: Option<String> = None, text;
        "speedStart" => speed_start: Option<String> = None, text;
        "speedEnd" => speed_end: Option<String> = None, text;
        "angleStart" => angle_start: Option<String> = None, text;
        "angleEnd" => angle_end: Option<String> = None, text;
        "distanceStart" => distance_start: Option<String> = None, text;
        "distanceEnd" => distance_end: Option<String> = None, text;
        "openPercentStart" => open_percent_start: Option<String> = None, text;
        "openPercentEnd" => open_percent_end: Option<String> = None, text;
        "frostIntensityStart" => frost_intensity_start: Option<String> = None, text;
        "frostIntensityEnd" => frost_intensity_end: Option<String> = None, text;
        "colorTemperatureStart" => color_temperature_start: Option<String> = None, text;
        "colorTemperatureEnd" => color_temperature_end: Option<String> = None, text;
        "duration" => duration: Option<String> = None, text;
        "durationStart" => duration_start: Option<String> = None, text;
        "durationEnd" => duration_end: Option<String> = None, text;
        "fogOutput" => fog_output: Option<String> = None, text;
        "fogOutputStart" => fog_output_start: Option<String> = None, text;
        "fogOutputEnd" => fog_output_end: Option<String> = None, text;
        "angle" => angle: Option<String> = None, text;
        "insertion" => insertion: Option<String> = None, text;
        "insertionStart" => insertion_start: Option<String> = None, text;
        "insertionEnd" => insertion_end: Option<String> = None, text;
        "shakeSpeed" => shake_speed: Option<String> = None, text;
        "shakeSpeedStart" => shake_speed_start: Option<String> = None, text;
        "shakeSpeedEnd" => shake_speed_end: Option<String> = None, text;
        "blade" => blade: Option<String> = None, text;
        "effectName" => effect_name: Option<String> = None, text;
        "hold" => hold: Option<String> = None, text;
        "comment" => comment: Option<String> = None, text;
        "colors" => colors: Option<Vec<String>> = None, list(text);
    }
}

bean! {
    /// The pixels of a profile (`FixtureMatrix`).
    pub struct FixtureMatrix {
        /// The keys of the pixels by Z, Y and X. None where there is no pixel.
        "pixelKeys" => pixel_keys: Option<Vec<Vec<Vec<Option<String>>>>> = None, list(list(list_opt(text)));
        /// The number of pixels on X, Y and Z, for a matrix without keys.
        "pixelCount" => pixel_count: Option<Vec<i32>> = None, list(int_w);
        /// The groups in the order the browser keeps them in (see [`read_pixel_groups`]).
        "pixelGroups" => pixel_groups: Option<Vec<FixtureMatrixPixelGroup>> = None, pixel_groups;
    }
}

/// A group of pixels of a matrix (`FixtureMatrixPixelGroup`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FixtureMatrixPixelGroup {
    pub name: String,
    /// All pixels of the matrix.
    pub is_all: bool,
    /// None for all pixels and for a group without a definition (which has no pixels).
    pub constraints: Option<FixtureMatrixPixelGroupConstraints>,
}

/// What the pixels of a group have to meet (`FixtureMatrixPixelGroupConstraints`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FixtureMatrixPixelGroupConstraints {
    /// The pixel keys themselves, in the order of the group.
    pub keys: Option<Vec<String>>,
    /// Rules on the position, e.g. "<=5", "even", "3n+1" or "2".
    pub x: Option<Vec<String>>,
    pub y: Option<Vec<String>>,
    pub z: Option<Vec<String>>,
    /// Regular expressions on the pixel key.
    pub name: Option<Vec<String>>,
}

fn pixel_groups(json: &Json, at: &At) -> Read<Vec<FixtureMatrixPixelGroup>> {
    match json {
        Json::Null => Ok(None),
        other => read_pixel_groups(other).map(Some).map_err(|message| at.error(&message)),
    }
}

// a name the browser takes for the index of an array (e.g. "2", but not "02" or "-1")
fn array_index(name: &str) -> Option<u64> {
    let canonical = name == "0" || (!name.starts_with('0') && !name.is_empty() && name.len() <= 10);
    if !canonical || !name.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    name.parse::<u64>().ok().filter(|index| *index <= 4_294_967_294)
}

/// The pixel groups of a matrix (`FixtureMatrixPixelGroupListDeserializer`), in the order the
/// browser keeps the names of an object in: the names of array indices ("1", "2") first and by
/// their number, all other names as they are written. Anything but an object has no groups.
pub fn read_pixel_groups(json: &Json) -> Result<Vec<FixtureMatrixPixelGroup>, String> {
    let properties = json.properties();
    let mut indices: Vec<(u64, &str, &Json)> = Vec::new();
    let mut names: Vec<(&str, &Json)> = Vec::new();
    for (name, value) in properties {
        match array_index(name) {
            Some(index) => indices.push((index, name, value)),
            None => names.push((name, value)),
        }
    }
    indices.sort_by_key(|(index, _, _)| *index);
    indices
        .into_iter()
        .map(|(_, name, value)| (name, value))
        .chain(names)
        .map(|(name, value)| read_pixel_group(name, Some(value)).map_err(|message| format!("{name}: {message}")))
        .collect()
}

// what the browser takes for false in a condition
fn is_falsy(json: Option<&Json>) -> bool {
    match json {
        None | Some(Json::Null) | Some(Json::Bool(false)) => true,
        Some(Json::Number(number)) => number.as_f64() == 0.0,
        Some(Json::String(text)) => text.is_empty(),
        _ => false,
    }
}

fn is_true(json: Option<&Json>) -> bool {
    matches!(json, Some(Json::Bool(true)))
}

fn texts_of(json: Option<&Json>) -> Result<Vec<String>, String> {
    match json {
        Some(Json::Array(items)) => items.iter().map(Json::as_text).collect(),
        _ => Ok(Vec::new()),
    }
}

/// A group like the fixture library defines it (`readGroup`): "all", an array of pixel keys or an
/// object of constraints, also wrapped as `{ isAll: <definition>, constraints: null }` like the
/// designers before stored it. A group without a definition has no pixels.
pub fn read_pixel_group(name: &str, data: Option<&Json>) -> Result<FixtureMatrixPixelGroup, String> {
    let mut group = FixtureMatrixPixelGroup {
        name: name.to_string(),
        is_all: false,
        constraints: None,
    };
    let mut data = data;
    while let Some(Json::Object(_)) = data {
        let Some(is_all) = data.and_then(|data| data.get("isAll")) else {
            break;
        };
        if is_true(Some(is_all)) {
            break;
        }
        data = if is_falsy(Some(is_all)) {
            data.and_then(|data| data.get("constraints"))
        } else {
            Some(is_all)
        };
    }
    if is_falsy(data) {
        return Ok(group);
    }
    let Some(data) = data else {
        return Ok(group);
    };
    if data.as_str() == Some("all") || is_true(data.get("isAll")) {
        group.is_all = true;
        return Ok(group);
    }
    let mut constraints = FixtureMatrixPixelGroupConstraints::default();
    match data {
        // an array of pixel keys
        Json::Array(_) => constraints.keys = Some(texts_of(Some(data))?),
        // an object of constraints (designers before also stored the empty keys)
        Json::Object(_) => {
            constraints.x = Some(texts_of(data.get("x"))?);
            constraints.y = Some(texts_of(data.get("y"))?);
            constraints.z = Some(texts_of(data.get("z"))?);
            constraints.name = Some(texts_of(data.get("name"))?);
            let keys = texts_of(data.get("keys"))?;
            if !keys.is_empty() {
                constraints.keys = Some(keys);
            }
        }
        _ => {}
    }
    group.constraints = Some(constraints);
    Ok(group)
}

bean! {
    /// The physical entry of a profile (`FixturePhysical`).
    pub struct FixturePhysical {
        /// In mm.
        "dimensions" => dimensions: Option<Vec<f64>> = None, list(num);
    }
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// Reads a project, like `DesignerJson.MAPPER.readValue(json, Project.class)`. The project is
/// taken as it is written: its version is not checked and nothing is migrated.
pub fn parse_project(json: &str) -> Result<Project, String> {
    Project::from_json(&Json::parse(json)?)
}

/// Reads a fixture profile.
pub fn parse_profile(json: &str) -> Result<FixtureProfile, String> {
    FixtureProfile::from_json(&Json::parse(json)?)
}

/// Replaces the profiles of a project which are named like the fixture library
/// (`{"$profile": "stairville/mh-100"}`) by the profile the passed function loads (its JSON), with
/// the name as its uuid (`resolveProfiles` of the conformance test of Java). The order of the
/// names is kept, which the profiles are played in: read the project with [`Json::parse`] and
/// hand the result to [`Project::from_json`].
pub fn resolve_profiles_json(project: &mut Json, load: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    if !matches!(project, Json::Object(_)) {
        return Err(format!("expected a project, not {}", kind(project)));
    }
    // like the loop over a JsonNode: the items of an array, the values of an object, or nothing
    let profiles: Vec<Json> = match project.get("fixtureProfiles") {
        Some(Json::Array(items)) => items.clone(),
        Some(profiles @ Json::Object(_)) => profiles
            .properties()
            .into_iter()
            .map(|(_, value)| value.clone())
            .collect(),
        _ => Vec::new(),
    };
    let mut resolved = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let Some(name) = profile.get("$profile") else {
            resolved.push(profile);
            continue;
        };
        let name = name.as_text().map_err(|message| format!("$profile: {message}"))?;
        let text = load(&name).ok_or_else(|| format!("the profile {name} is missing"))?;
        let mut library_profile = Json::parse(&text).map_err(|message| format!("the profile {name}: {message}"))?;
        if !matches!(library_profile, Json::Object(_)) {
            return Err(format!(
                "the profile {name} is {}, not an object",
                kind(&library_profile)
            ));
        }
        library_profile.set("uuid", Json::String(name));
        resolved.push(library_profile);
    }
    project.set("fixtureProfiles", Json::Array(resolved));
    Ok(())
}

/// The same as [`resolve_profiles_json`] on a value of `serde_json`. Mind that such a value has
/// lost the order of its names (the pixel groups of a matrix, the channels and the wheels of a
/// profile), unless `serde_json` is built with `preserve_order`, and that it reads the numbers
/// less exactly than Java does: prefer [`resolve_profiles_json`].
pub fn resolve_profiles(project: &mut serde_json::Value, load: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    let serde_json::Value::Object(properties) = project else {
        return Err("expected a project, not anything else".to_string());
    };
    let profiles: Vec<serde_json::Value> = match properties.get("fixtureProfiles") {
        Some(serde_json::Value::Array(items)) => items.clone(),
        Some(serde_json::Value::Object(entries)) => entries.values().cloned().collect(),
        _ => Vec::new(),
    };
    let mut resolved = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let Some(name) = profile.get("$profile") else {
            resolved.push(profile);
            continue;
        };
        let name = Json::from(name)
            .as_text()
            .map_err(|message| format!("$profile: {message}"))?;
        let text = load(&name).ok_or_else(|| format!("the profile {name} is missing"))?;
        let mut library_profile: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| format!("the profile {name}: {error}"))?;
        match &mut library_profile {
            serde_json::Value::Object(entries) => {
                entries.insert("uuid".to_string(), serde_json::Value::String(name));
            }
            _ => return Err(format!("the profile {name} is not an object")),
        }
        resolved.push(library_profile);
    }
    properties.insert("fixtureProfiles".to_string(), serde_json::Value::Array(resolved));
    Ok(())
}
