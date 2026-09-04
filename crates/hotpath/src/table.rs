//! Minimal ASCII table renderer for the text report.
//!
//! Renders the same `+---+---+` / `| a | b |` layout prettytable-rs produced
//! with its default format (one padding space on each side, a separator line
//! between every row), so report screenshots and text-parsing tests are
//! unaffected. Only the features the reports use are implemented: left-aligned
//! cells, multi-line cells, and bold/colored cells rendered with ANSI escapes.

use std::fmt;
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Cyan,
    Red,
}

impl Color {
    fn sgr(self) -> &'static str {
        match self {
            Color::Cyan => "36",
            Color::Red => "31",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cell {
    lines: Vec<String>,
    width: usize,
    bold: bool,
    color: Option<Color>,
}

impl Cell {
    /// Left-aligned cell; embedded newlines produce a multi-line cell.
    pub fn new(text: &str) -> Self {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let width = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
        Cell {
            lines,
            width,
            bold: false,
            color: None,
        }
    }

    /// Bold cyan cell, the style every report uses for its header row.
    pub fn header(text: &str) -> Self {
        Cell::new(text).bold().color(Color::Cyan)
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    fn line(&self, idx: usize) -> &str {
        self.lines.get(idx).map_or("", String::as_str)
    }

    fn styled(&self) -> bool {
        self.bold || self.color.is_some()
    }

    fn write_style_prefix(&self, out: &mut dyn Write) -> io::Result<()> {
        let mut codes: Vec<&str> = Vec::with_capacity(2);
        if self.bold {
            codes.push("1");
        }
        if let Some(color) = self.color {
            codes.push(color.sgr());
        }
        write!(out, "\x1b[{}m", codes.join(";"))
    }
}

#[derive(Debug, Default)]
pub struct Table {
    rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn new() -> Self {
        Table::default()
    }

    pub fn add_row(&mut self, cells: Vec<Cell>) {
        self.rows.push(cells);
    }

    /// Writes the table to `out`. Cell styles are emitted as ANSI escapes only
    /// when `colors` is true; the plain layout is byte-identical either way.
    pub fn print(&self, out: &mut dyn Write, colors: bool) -> io::Result<()> {
        let col_widths = self.column_widths();
        let separator = separator_line(&col_widths);

        out.write_all(separator.as_bytes())?;
        for row in &self.rows {
            let height = row.iter().map(|c| c.lines.len()).max().unwrap_or(0).max(1);
            for line_idx in 0..height {
                out.write_all(b"|")?;
                for (col, width) in col_widths.iter().enumerate() {
                    let cell = row.get(col);
                    let text = cell.map_or("", |c| c.line(line_idx));
                    let fill = width.saturating_sub(display_width(text));
                    let styled = colors && cell.is_some_and(Cell::styled);
                    if styled {
                        cell.unwrap().write_style_prefix(out)?;
                    }
                    write!(out, " {text}{} ", " ".repeat(fill))?;
                    if styled {
                        out.write_all(b"\x1b[0m")?;
                    }
                    out.write_all(b"|")?;
                }
                out.write_all(b"\n")?;
            }
            out.write_all(separator.as_bytes())?;
        }
        Ok(())
    }

    fn column_widths(&self) -> Vec<usize> {
        let columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0; columns];
        for row in &self.rows {
            for (col, cell) in row.iter().enumerate() {
                widths[col] = widths[col].max(cell.width);
            }
        }
        widths
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = Vec::new();
        self.print(&mut buf, false).map_err(|_| fmt::Error)?;
        f.write_str(&String::from_utf8_lossy(&buf))
    }
}

fn separator_line(col_widths: &[usize]) -> String {
    let mut line = String::from("+");
    for width in col_widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line.push('\n');
    line
}

/// Terminal column count of `text`. Report cells are ASCII identifiers and
/// numbers, so this only approximates Unicode width: zero-width marks count 0,
/// East Asian wide characters and emoji count 2, everything else 1.
fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(c: char) -> usize {
    match c as u32 {
        0x0300..=0x036F | 0x200B..=0x200F | 0xFE00..=0xFE0F | 0x20D0..=0x20FF => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F191..=0x1F19A
        | 0x1F300..=0x1F64F
        | 0x1F680..=0x1F6FF
        | 0x1F900..=0x1F9FF
        | 0x1FA70..=0x1FAFF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(table: &Table, colors: bool) -> String {
        let mut buf = Vec::new();
        table.print(&mut buf, colors).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn matches_prettytable_default_format() {
        let mut table = Table::new();
        table.add_row(vec![
            Cell::header("Function"),
            Cell::header("Calls"),
            Cell::header("% Total"),
        ]);
        table.add_row(vec![
            Cell::new("main::run"),
            Cell::new("12"),
            Cell::new("100.00%"),
        ]);
        table.add_row(vec![Cell::new("x"), Cell::new(""), Cell::new("0.5%")]);
        let expected = "\
+-----------+-------+---------+
| Function  | Calls | % Total |
+-----------+-------+---------+
| main::run | 12    | 100.00% |
+-----------+-------+---------+
| x         |       | 0.5%    |
+-----------+-------+---------+
";
        assert_eq!(render(&table, false), expected);
        assert_eq!(table.to_string(), expected);
    }

    #[test]
    fn multiline_cell_grows_row_height() {
        let mut table = Table::new();
        table.add_row(vec![Cell::new("Error")]);
        table.add_row(vec![Cell::new("line one\nline two longer\n")]);
        let expected = "\
+-----------------+
| Error           |
+-----------------+
| line one        |
| line two longer |
+-----------------+
";
        assert_eq!(render(&table, false), expected);
    }

    #[test]
    fn ragged_rows_pad_missing_cells() {
        let mut table = Table::new();
        table.add_row(vec![Cell::new("a"), Cell::new("b"), Cell::new("c")]);
        table.add_row(vec![Cell::new("only")]);
        let expected = "\
+------+---+---+
| a    | b | c |
+------+---+---+
| only |   |   |
+------+---+---+
";
        assert_eq!(render(&table, false), expected);
    }

    #[test]
    fn empty_cells_keep_padding() {
        let mut table = Table::new();
        table.add_row(vec![Cell::new(""), Cell::new("")]);
        assert_eq!(render(&table, false), "+--+--+\n|  |  |\n+--+--+\n");
    }

    #[test]
    fn wide_characters_align_columns() {
        let mut table = Table::new();
        table.add_row(vec![Cell::new("Thread"), Cell::new("Alloc")]);
        table.add_row(vec![Cell::new("🆕 worker"), Cell::new("日本語")]);
        table.add_row(vec![Cell::new("🗑\u{FE0F} old"), Cell::new("1 KB")]);
        let expected = "\
+-----------+--------+
| Thread    | Alloc  |
+-----------+--------+
| 🆕 worker | 日本語 |
+-----------+--------+
| 🗑\u{FE0F} old    | 1 KB   |
+-----------+--------+
";
        assert_eq!(render(&table, false), expected);
    }

    #[test]
    fn colors_wrap_only_styled_cells() {
        let mut table = Table::new();
        table.add_row(vec![Cell::header("Name"), Cell::new("plain").bold()]);
        table.add_row(vec![Cell::new("v"), Cell::new("err").color(Color::Red)]);
        let expected = "\
+------+-------+
|\x1b[1;36m Name \x1b[0m|\x1b[1m plain \x1b[0m|
+------+-------+
| v    |\x1b[31m err   \x1b[0m|
+------+-------+
";
        assert_eq!(render(&table, true), expected);
        assert!(!render(&table, false).contains('\x1b'));
    }
}
