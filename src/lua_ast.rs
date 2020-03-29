//! Defines module for defining a small Lua AST for simple codegen.

use std::{
    fmt::{self, Write},
    num::FpCategory,
};

/// Trait that helps turn a type into an equivalent Lua snippet.
///
/// Designed to be similar to the `Display` trait from Rust's std.
trait FmtLua {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result;

    /// Used to override how this type will appear when used as a table key.
    /// Some types, like strings, can have a shorter representation as a table
    /// key than the default, safe approach.
    fn fmt_table_key(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        write!(output, "[")?;
        self.fmt_lua(output)?;
        write!(output, "]")
    }
}

pub(crate) enum Statement {
    Return(Expression),
}

impl FmtLua for Statement {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        match self {
            Self::Return(literal) => {
                write!(output, "return ")?;
                literal.fmt_lua(output)
            }
        }
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, output: &mut fmt::Formatter) -> fmt::Result {
        let mut stream = LuaStream::new(output);
        FmtLua::fmt_lua(self, &mut stream)
    }
}

pub(crate) enum Expression {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
    Table(Table),

    /// Arrays are not technically distinct from other tables in Lua, but this
    /// representation is more convenient.
    Array(Vec<Expression>),
}

impl Expression {
    pub fn table(entries: Vec<(Expression, Expression)>) -> Self {
        Self::Table(Table { entries })
    }
}

impl FmtLua for Expression {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(output, "nil"),
            Self::Bool(inner) => inner.fmt_lua(output),
            Self::Number(inner) => inner.fmt_lua(output),
            Self::String(inner) => inner.fmt_lua(output),
            Self::Table(inner) => inner.fmt_lua(output),
            Self::Array(inner) => inner.fmt_lua(output),
        }
    }

    fn fmt_table_key(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        match self {
            Self::Nil => panic!("nil cannot be a table key"),
            Self::Bool(inner) => inner.fmt_table_key(output),
            Self::Number(inner) => inner.fmt_table_key(output),
            Self::String(inner) => inner.fmt_table_key(output),
            Self::Table(inner) => inner.fmt_table_key(output),
            Self::Array(inner) => inner.fmt_table_key(output),
        }
    }
}

impl From<String> for Expression {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&'_ str> for Expression {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<Table> for Expression {
    fn from(value: Table) -> Self {
        Self::Table(value)
    }
}

impl FmtLua for bool {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        write!(output, "{}", self)
    }
}

impl FmtLua for f64 {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        match self.classify() {
            FpCategory::Nan => write!(output, "0/0"),
            FpCategory::Infinite => {
                if self.is_sign_positive() {
                    write!(output, "math.huge")
                } else {
                    write!(output, "-math.huge")
                }
            }
            _ => write!(output, "{}", self),
        }
    }
}

impl FmtLua for String {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        write!(output, "\"{}\"", self)
    }

    fn fmt_table_key(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        if is_valid_ident(self) {
            write!(output, "{}", self)
        } else {
            write!(output, "[\"{}\"]", self)
        }
    }
}

impl FmtLua for Vec<Expression> {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        write!(output, "{{")?;

        for (index, value) in self.iter().enumerate() {
            value.fmt_lua(output)?;

            if index < self.len() - 1 {
                write!(output, ", ")?;
            }
        }

        write!(output, "}}")
    }
}

pub(crate) struct Table {
    pub entries: Vec<(Expression, Expression)>,
}

impl FmtLua for Table {
    fn fmt_lua(&self, output: &mut LuaStream<'_>) -> fmt::Result {
        writeln!(output, "{{")?;
        output.indent();

        for (key, value) in &self.entries {
            key.fmt_table_key(output)?;
            write!(output, " = ")?;
            value.fmt_lua(output)?;
            writeln!(output, ",")?;
        }

        output.unindent();
        write!(output, "}}")
    }
}

fn is_valid_ident_char_start(value: char) -> bool {
    value.is_ascii_alphabetic() || value == '_'
}

fn is_valid_ident_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || value == '_'
}

fn is_keyword(value: &str) -> bool {
    match value {
        "and" | "break" | "do" | "else" | "elseif" | "end" | "false" | "for" | "function"
        | "if" | "in" | "local" | "nil" | "not" | "or" | "repeat" | "return" | "then" | "true"
        | "until" | "while" => true,
        _ => false,
    }
}

/// Tells whether the given string is a valid Lua identifier.
fn is_valid_ident(value: &str) -> bool {
    if is_keyword(value) {
        return false;
    }

    let mut chars = value.chars();

    match chars.next() {
        Some(first) => {
            if !is_valid_ident_char_start(first) {
                return false;
            }
        }
        None => return false,
    }

    chars.all(is_valid_ident_char)
}

/// Wraps a `fmt::Write` with additional tracking to do pretty-printing of Lua.
///
/// Behaves similarly to `fmt::Formatter`. This trait's relationship to `LuaFmt`
/// is very similar to `Formatter`'s relationship to `Display`.
struct LuaStream<'a> {
    indent_level: usize,
    is_start_of_line: bool,
    inner: &'a mut (dyn fmt::Write + 'a),
}

impl fmt::Write for LuaStream<'_> {
    /// Method to support the `write!` and `writeln!` macros. Instead of using a
    /// trait directly, these macros just call `write_str` on their first
    /// argument.
    ///
    /// This method is also available on `io::Write` and `fmt::Write`.
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let mut is_first_line = true;

        for line in value.split('\n') {
            if is_first_line {
                is_first_line = false;
            } else {
                self.line()?;
            }

            if !line.is_empty() {
                if self.is_start_of_line {
                    self.is_start_of_line = false;
                    let indentation = "\t".repeat(self.indent_level);
                    self.inner.write_str(&indentation)?;
                }

                self.inner.write_str(line)?;
            }
        }

        Ok(())
    }
}

impl<'a> LuaStream<'a> {
    fn new(inner: &'a mut (dyn fmt::Write + 'a)) -> Self {
        LuaStream {
            indent_level: 0,
            is_start_of_line: true,
            inner,
        }
    }

    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn unindent(&mut self) {
        assert!(self.indent_level > 0);
        self.indent_level -= 1;
    }

    fn line(&mut self) -> fmt::Result {
        self.is_start_of_line = true;
        self.inner.write_str("\n")
    }
}
