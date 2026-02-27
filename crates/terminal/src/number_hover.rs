use alacritty_terminal::{
    Term,
    event::EventListener,
    grid::Dimensions,
    index::{Column, Point as AlacPoint},
};
use regex::Regex;
use std::ops::{Index, RangeInclusive};
use std::sync::LazyLock;

/// The format of the detected number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberFormat {
    Decimal,
    Hexadecimal,
    Binary,
    Octal,
}

/// A parsed number with its original string representation and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNumber {
    /// The original string as it appears in the terminal.
    pub original: String,
    /// The parsed numeric value.
    pub value: i128,
    /// The detected format of the number.
    pub format: NumberFormat,
    /// The range in the terminal grid where the number is located.
    pub word_match: RangeInclusive<AlacPoint>,
}

impl ParsedNumber {
    /// Format as binary with 4-bit grouping and position markers.
    /// Returns (binary_string, bit_positions).
    /// Position markers use regular ASCII digits (displayed in smaller font in UI).
    /// Position markers only show the lowest bit position of each 4-bit group (0, 4, 8, 12, ...).
    pub fn format_as_binary(&self) -> (String, String) {
        let abs_value = self.value.unsigned_abs();

        // Determine the number of bits needed (align to 8, 16, 32, or 64)
        let bit_count = if abs_value == 0 {
            8
        } else {
            let needed = 128 - abs_value.leading_zeros() as usize;
            if needed <= 8 {
                8
            } else if needed <= 16 {
                16
            } else if needed <= 32 {
                32
            } else {
                64
            }
        };

        // Format binary with 4-bit grouping
        let mut binary_str = String::with_capacity(bit_count + bit_count / 4);
        let mut positions_str = String::with_capacity(bit_count + bit_count / 4);
        let num_groups = bit_count / 4;

        // Add prefix padding for positions string to align with "0b" or "-0b"
        let prefix = if self.value < 0 { "-0b" } else { "0b" };
        for _ in 0..prefix.len() {
            positions_str.push(' ');
        }

        for group_idx in 0..num_groups {
            let group_start_bit = bit_count - 1 - group_idx * 4;

            // Add 4 bits for this group
            for offset in 0..4 {
                let bit_pos = group_start_bit - offset;
                let bit = (abs_value >> bit_pos) & 1;
                binary_str.push(if bit == 1 { '1' } else { '0' });
            }

            // Position marker: use regular ASCII digit, right-aligned within 4-char group
            let lowest_bit = group_start_bit - 3;
            let pos_str = lowest_bit.to_string();
            // Pad to align with 4-character binary group
            for _ in 0..(4 - pos_str.len()) {
                positions_str.push(' ');
            }
            positions_str.push_str(&pos_str);

            // Add space between groups (except after the last group)
            if group_idx < num_groups - 1 {
                binary_str.push(' ');
                positions_str.push(' ');
            }
        }

        (format!("{}{}", prefix, binary_str), positions_str)
    }

    /// Format as decimal.
    pub fn format_as_decimal(&self) -> String {
        format_with_separators(self.value)
    }

    /// Format as hexadecimal.
    pub fn format_as_hex(&self) -> String {
        if self.value < 0 {
            format!("-0x{:X}", self.value.unsigned_abs())
        } else {
            format!("0x{:X}", self.value)
        }
    }

    /// Format as octal.
    pub fn format_as_octal(&self) -> String {
        if self.value < 0 {
            format!("-0o{:o}", self.value.unsigned_abs())
        } else {
            format!("0o{:o}", self.value)
        }
    }
}


/// Format a number with thousand separators.
fn format_with_separators(n: i128) -> String {
    let is_negative = n < 0;
    let abs_str = n.unsigned_abs().to_string();
    let chars: Vec<char> = abs_str.chars().collect();
    let mut result = String::with_capacity(abs_str.len() + abs_str.len() / 3);

    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*c);
    }

    if is_negative {
        format!("-{}", result)
    } else {
        result
    }
}

// Regex patterns for number detection
static HEX_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^0[xX][0-9a-fA-F][0-9a-fA-F_]*$").unwrap());
static BIN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^0[bB][01][01_]*$").unwrap());
static OCT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^0[oO][0-7][0-7_]*$").unwrap());
static DEC_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-?[0-9][0-9_]*$").unwrap());

/// Parse a string as a number if it matches any supported format.
pub fn parse_number_string(s: &str) -> Option<(i128, NumberFormat)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Try hexadecimal (0x...)
    if HEX_REGEX.is_match(s) {
        let hex_str = s[2..].replace('_', "");
        if let Ok(value) = i128::from_str_radix(&hex_str, 16) {
            return Some((value, NumberFormat::Hexadecimal));
        }
    }

    // Try binary (0b...)
    if BIN_REGEX.is_match(s) {
        let bin_str = s[2..].replace('_', "");
        if let Ok(value) = i128::from_str_radix(&bin_str, 2) {
            return Some((value, NumberFormat::Binary));
        }
    }

    // Try octal (0o...)
    if OCT_REGEX.is_match(s) {
        let oct_str = s[2..].replace('_', "");
        if let Ok(value) = i128::from_str_radix(&oct_str, 8) {
            return Some((value, NumberFormat::Octal));
        }
    }

    // Try decimal
    if DEC_REGEX.is_match(s) {
        let dec_str = s.replace('_', "");
        if let Ok(value) = dec_str.parse::<i128>() {
            return Some((value, NumberFormat::Decimal));
        }
    }

    None
}

/// Check if a character is part of a number.
fn is_number_char(c: char) -> bool {
    c.is_ascii_hexdigit() || c == 'x' || c == 'X' || c == 'b' || c == 'B' || c == 'o' || c == 'O' || c == '_' || c == '-'
}

/// Find a number at the given terminal grid position.
pub fn find_number_at_position<T: EventListener>(
    term: &Term<T>,
    point: AlacPoint,
) -> Option<ParsedNumber> {
    let grid = term.grid();
    let line = point.line;
    let col = point.column;

    // Check if we're on a valid character
    let cell = grid.index(point);
    let c = cell.c;

    // Must start with a digit or minus sign (for negative numbers)
    if !c.is_ascii_digit() && c != '-' {
        return None;
    }

    let num_cols = grid.columns();

    // Search left to find the start of the number
    let mut start_col = col.0;
    while start_col > 0 {
        let prev_point = AlacPoint::new(line, Column(start_col - 1));
        let prev_c = grid.index(prev_point).c;
        if is_number_char(prev_c) || prev_c.is_ascii_digit() {
            start_col -= 1;
        } else {
            break;
        }
    }

    // Search right to find the end of the number
    let mut end_col = col.0;
    while end_col < num_cols - 1 {
        let next_point = AlacPoint::new(line, Column(end_col + 1));
        let next_c = grid.index(next_point).c;
        if is_number_char(next_c) || next_c.is_ascii_digit() {
            end_col += 1;
        } else {
            break;
        }
    }

    // Extract the number string
    let mut number_str = String::with_capacity(end_col - start_col + 1);
    for col_idx in start_col..=end_col {
        let pt = AlacPoint::new(line, Column(col_idx));
        number_str.push(grid.index(pt).c);
    }

    // Try to parse the number
    if let Some((value, format)) = parse_number_string(&number_str) {
        let word_match = AlacPoint::new(line, Column(start_col))
            ..=AlacPoint::new(line, Column(end_col));

        return Some(ParsedNumber {
            original: number_str,
            value,
            format,
            word_match,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decimal() {
        assert_eq!(
            parse_number_string("123"),
            Some((123, NumberFormat::Decimal))
        );
        assert_eq!(
            parse_number_string("-456"),
            Some((-456, NumberFormat::Decimal))
        );
        assert_eq!(
            parse_number_string("1_000_000"),
            Some((1_000_000, NumberFormat::Decimal))
        );
    }

    #[test]
    fn test_parse_hex() {
        assert_eq!(
            parse_number_string("0xFF"),
            Some((255, NumberFormat::Hexadecimal))
        );
        assert_eq!(
            parse_number_string("0xDEADBEEF"),
            Some((0xDEADBEEF, NumberFormat::Hexadecimal))
        );
        assert_eq!(
            parse_number_string("0x1_0000"),
            Some((0x10000, NumberFormat::Hexadecimal))
        );
    }

    #[test]
    fn test_parse_binary() {
        assert_eq!(
            parse_number_string("0b1010"),
            Some((10, NumberFormat::Binary))
        );
        assert_eq!(
            parse_number_string("0B1111_0000"),
            Some((0xF0, NumberFormat::Binary))
        );
    }

    #[test]
    fn test_parse_octal() {
        assert_eq!(
            parse_number_string("0o777"),
            Some((0o777, NumberFormat::Octal))
        );
        assert_eq!(
            parse_number_string("0O123"),
            Some((0o123, NumberFormat::Octal))
        );
    }

    #[test]
    fn test_format_binary() {
        let num = ParsedNumber {
            original: "255".to_string(),
            value: 255,
            format: NumberFormat::Decimal,
            word_match: AlacPoint::new(alacritty_terminal::index::Line(0), Column(0))
                ..=AlacPoint::new(alacritty_terminal::index::Line(0), Column(2)),
        };
        let (bin, pos) = num.format_as_binary();
        assert_eq!(bin, "0b1111 1111");
        // Positions: 2 spaces for "0b" prefix + right-aligned position numbers
        // "0b" + "1111" + " " + "1111"
        // "  " + "   4" + " " + "   0"
        assert_eq!(pos, "     4    0");
    }

    #[test]
    fn test_format_binary_16bit() {
        let num = ParsedNumber {
            original: "0xABCD".to_string(),
            value: 0xABCD,
            format: NumberFormat::Hexadecimal,
            word_match: AlacPoint::new(alacritty_terminal::index::Line(0), Column(0))
                ..=AlacPoint::new(alacritty_terminal::index::Line(0), Column(5)),
        };
        let (bin, pos) = num.format_as_binary();
        assert_eq!(bin, "0b1010 1011 1100 1101");
        // Positions: 2 spaces for "0b" prefix + 12, 8, 4, 0
        assert_eq!(pos, "    12    8    4    0");
    }

    #[test]
    fn test_format_binary_32bit() {
        let num = ParsedNumber {
            original: "0x12345678".to_string(),
            value: 0x12345678,
            format: NumberFormat::Hexadecimal,
            word_match: AlacPoint::new(alacritty_terminal::index::Line(0), Column(0))
                ..=AlacPoint::new(alacritty_terminal::index::Line(0), Column(9)),
        };
        let (bin, pos) = num.format_as_binary();
        assert_eq!(bin, "0b0001 0010 0011 0100 0101 0110 0111 1000");
        // Positions: 2 spaces for "0b" prefix + 28, 24, 20, 16, 12, 8, 4, 0
        assert_eq!(pos, "    28   24   20   16   12    8    4    0");
    }

    #[test]
    fn test_format_binary_negative() {
        let num = ParsedNumber {
            original: "-255".to_string(),
            value: -255,
            format: NumberFormat::Decimal,
            word_match: AlacPoint::new(alacritty_terminal::index::Line(0), Column(0))
                ..=AlacPoint::new(alacritty_terminal::index::Line(0), Column(3)),
        };
        let (bin, pos) = num.format_as_binary();
        assert_eq!(bin, "-0b1111 1111");
        // Positions: 3 spaces for "-0b" prefix + right-aligned position numbers
        assert_eq!(pos, "      4    0");
    }

    #[test]
    fn test_format_decimal_with_separators() {
        let num = ParsedNumber {
            original: "1000000".to_string(),
            value: 1_000_000,
            format: NumberFormat::Decimal,
            word_match: AlacPoint::new(alacritty_terminal::index::Line(0), Column(0))
                ..=AlacPoint::new(alacritty_terminal::index::Line(0), Column(6)),
        };
        assert_eq!(num.format_as_decimal(), "1,000,000");
    }

    #[test]
    fn test_format_hex() {
        let num = ParsedNumber {
            original: "255".to_string(),
            value: 255,
            format: NumberFormat::Decimal,
            word_match: AlacPoint::new(alacritty_terminal::index::Line(0), Column(0))
                ..=AlacPoint::new(alacritty_terminal::index::Line(0), Column(2)),
        };
        assert_eq!(num.format_as_hex(), "0xFF");
    }

}
