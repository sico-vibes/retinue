//! A small ANSI screen model for the terminal dock: enough of a VT to read a
//! shell, not a full emulator.
//!
//! It handles printable text, wrapping, `\r`, `\n`, backspace, tab, a scrolling
//! window with scrollback, the cursor, erase-in-line and erase-in-display, and
//! `SGR` colours and emphasis. Anything else in an escape sequence is consumed
//! and ignored, so an unknown sequence can never reach the screen as text.

/// One of the sixteen colours a terminal draws, or the theme's own text colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Colour {
    #[default]
    Default,
    /// An entry of the xterm palette: 0 to 15 are the look's own sixteen, and
    /// 16 to 255 are the standard cube and grey ramp.
    Indexed(u8),
    /// A true-colour value, drawn as it is.
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Colour,
    pub bg: Colour,
    pub bold: bool,
    pub dim: bool,
    pub inverse: bool,
}

impl Style {
    pub fn is_plain(&self) -> bool {
        *self == Style::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', style: Style::default() }
    }
}

/// How much scrollback is kept. A build log is long; a terminal that keeps every
/// line of it is a memory leak with a cursor.
pub const SCROLLBACK_LIMIT: usize = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    Csi,
    /// Inside an OSC string, waiting for BEL or ST.
    Osc,
}

pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<Cell>,
    scrollback: Vec<Vec<Cell>>,
    row: usize,
    col: usize,
    style: Style,
    state: State,
    /// The CSI parameters being collected.
    params: Vec<u16>,
    /// A private marker (`?`, `>`, `<`, `=`) on the sequence being read.
    private: Option<char>,
    cursor_visible: bool,
    saved: (usize, usize),
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Screen {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            scrollback: Vec::new(),
            row: 0,
            col: 0,
            style: Style::default(),
            state: State::Ground,
            params: Vec::new(),
            private: None,
            cursor_visible: true,
            saved: (0, 0),
        }
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    fn blank_row(&self) -> Vec<Cell> {
        vec![Cell::default(); self.cols]
    }

    /// Grow or shrink to a new grid, keeping the text that still fits.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut cells = vec![Cell::default(); cols * rows];
        for row in 0..rows.min(self.rows) {
            for col in 0..cols.min(self.cols) {
                cells[row * cols + col] = self.cells[row * self.cols + col];
            }
        }
        self.cells = cells;
        self.cols = cols;
        self.rows = rows;
        self.row = self.row.min(rows - 1);
        self.col = self.col.min(cols - 1);
    }

    fn clear_line(&mut self, from: usize, to: usize) {
        let row = self.row;
        for col in from..to.min(self.cols) {
            self.cells[row * self.cols + col] = Cell::default();
        }
    }

    fn newline(&mut self) {
        if self.row + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.row += 1;
        }
    }

    fn scroll_up(&mut self) {
        let first: Vec<Cell> = self.cells[..self.cols].to_vec();
        self.cells.drain(..self.cols);
        let blank = self.blank_row();
        self.cells.extend_from_slice(&blank);
        self.scrollback.push(first);
        if self.scrollback.len() > SCROLLBACK_LIMIT {
            self.scrollback.remove(0);
        }
        self.row = self.rows - 1;
    }

    fn put(&mut self, ch: char) {
        if self.col >= self.cols {
            self.col = 0;
            self.newline();
        }
        let style = self.style;
        self.cells[self.row * self.cols + self.col] = Cell { ch, style };
        self.col += 1;
    }

    pub fn feed(&mut self, text: &str) {
        for ch in text.chars() {
            self.step(ch);
        }
    }

    fn step(&mut self, ch: char) {
        match self.state {
            State::Ground => self.ground(ch),
            State::Escape => match ch {
                '[' => {
                    self.state = State::Csi;
                    self.params.clear();
                    self.private = None;
                }
                ']' => self.state = State::Osc,
                // Two-character sequences: charset selection, keypad modes and
                // the like. They change nothing this screen draws.
                '(' | ')' | '*' | '+' | '#' | '%' => self.state = State::Escape,
                '7' => {
                    self.saved = (self.row, self.col);
                    self.state = State::Ground;
                }
                '8' => {
                    self.row = self.saved.0.min(self.rows - 1);
                    self.col = self.saved.1.min(self.cols - 1);
                    self.state = State::Ground;
                }
                'D' => {
                    self.newline();
                    self.state = State::Ground;
                }
                'M' => {
                    if self.row == 0 {
                        let blank = self.blank_row();
                        self.cells.extend_from_slice(&blank);
                        self.cells.drain(..self.cols);
                        self.scrollback.push(self.blank_row());
                    } else {
                        self.row -= 1;
                    }
                    self.state = State::Ground;
                }
                _ => self.state = State::Ground,
            },
            State::Osc => match ch {
                '\u{7}' => self.state = State::Ground,
                '\u{1b}' => self.state = State::Ground,
                _ => {}
            },
            State::Csi => {
                if ch.is_ascii_digit() {
                    match self.params.last_mut() {
                        Some(value) => *value = value.saturating_mul(10).saturating_add(ch as u16 - '0' as u16),
                        None => self.params.push(ch as u16 - '0' as u16),
                    }
                    return;
                }
                if ch == ';' {
                    self.params.push(0);
                    return;
                }
                if matches!(ch, '?' | '>' | '<' | '=') {
                    self.private = Some(ch);
                    return;
                }
                let params = self.params.clone();
                self.state = State::Ground;
                self.csi(ch, &params);
            }
        }
    }

    fn ground(&mut self, ch: char) {
        match ch {
            '\u{1b}' => self.state = State::Escape,
            '\r' => self.col = 0,
            '\n' => {
                self.newline();
                self.col = 0;
            }
            '\u{8}' => {
                if self.col > 0 {
                    self.col -= 1;
                }
            }
            '\t' => {
                let next = (self.col / 8 + 1) * 8;
                self.col = next.min(self.cols - 1);
            }
            '\u{7}' => {}
            ch if (ch as u32) < 0x20 => {}
            ch => self.put(ch),
        }
    }

    fn csi(&mut self, final_char: char, params: &[u16]) {
        let first = params.first().copied().unwrap_or(0);
        let count = |value: u16| if value == 0 { 1 } else { value as usize };
        match final_char {
            'm' => self.sgr(params),
            'A' => self.row = self.row.saturating_sub(count(first)),
            'B' => self.row = (self.row + count(first)).min(self.rows - 1),
            'C' => self.col = (self.col + count(first)).min(self.cols - 1),
            'D' => self.col = self.col.saturating_sub(count(first)),
            'E' => {
                self.row = (self.row + count(first)).min(self.rows - 1);
                self.col = 0;
            }
            'F' => {
                self.row = self.row.saturating_sub(count(first));
                self.col = 0;
            }
            'G' => self.col = (count(first) - 1).min(self.cols - 1),
            'd' => self.row = (count(first) - 1).min(self.rows - 1),
            'H' | 'f' => {
                let row = params.first().copied().unwrap_or(1);
                let col = params.get(1).copied().unwrap_or(1);
                self.row = (count(row) - 1).min(self.rows - 1);
                self.col = (count(col) - 1).min(self.cols - 1);
            }
            'K' => match first {
                1 => self.clear_line(0, self.col + 1),
                2 => self.clear_line(0, self.cols),
                _ => self.clear_line(self.col, self.cols),
            },
            'J' => match first {
                1 => {
                    for col in 0..self.col + 1 {
                        self.cells[self.row * self.cols + col] = Cell::default();
                    }
                    for row in 0..self.row {
                        for col in 0..self.cols {
                            self.cells[row * self.cols + col] = Cell::default();
                        }
                    }
                }
                2 | 3 => {
                    self.cells = vec![Cell::default(); self.cols * self.rows];
                    self.row = 0;
                    self.col = 0;
                }
                _ => {
                    for col in self.col..self.cols {
                        self.cells[self.row * self.cols + col] = Cell::default();
                    }
                    for row in self.row + 1..self.rows {
                        for col in 0..self.cols {
                            self.cells[row * self.cols + col] = Cell::default();
                        }
                    }
                }
            },
            'h' | 'l' => {
                if self.private == Some('?') && params.contains(&25) {
                    self.cursor_visible = final_char == 'h';
                }
            }
            _ => {}
        }
        if final_char != 'm' {
            self.params = params.to_vec();
            self.params.clear();
        }
    }

    fn sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.style = Style::default();
            return;
        }
        let mut index = 0;
        while index < params.len() {
            let code = params[index];
            match code {
                0 => self.style = Style::default(),
                1 => self.style.bold = true,
                2 => self.style.dim = true,
                3 => self.style.bold = false,
                4 => {}
                7 => self.style.inverse = true,
                9 => {}
                22 => {
                    self.style.bold = false;
                    self.style.dim = false;
                }
                23 => {}
                24 => {}
                27 => self.style.inverse = false,
                29 => {}
                30..=37 => self.style.fg = Colour::Indexed((code - 30) as u8),
                39 => self.style.fg = Colour::Default,
                40..=47 => self.style.bg = Colour::Indexed((code - 40) as u8),
                49 => self.style.bg = Colour::Default,
                90..=97 => self.style.fg = Colour::Indexed((code - 90 + 8) as u8),
                100..=107 => self.style.bg = Colour::Indexed((code - 100 + 8) as u8),
                38 | 48 => {
                    // Extended colour: `5;n` is an xterm index, `2;r;g;b` is
                    // true colour. Both are kept as they are and drawn by name.
                    let target = code == 38;
                    let colour = match params.get(index + 1).copied() {
                        Some(5) => params.get(index + 2).copied().map(|n| Colour::Indexed(n.min(255) as u8)),
                        Some(2) => {
                            let r = params.get(index + 2).copied().unwrap_or(0) as u8;
                            let g = params.get(index + 3).copied().unwrap_or(0) as u8;
                            let b = params.get(index + 4).copied().unwrap_or(0) as u8;
                            Some(Colour::Rgb(r, g, b))
                        }
                        _ => None,
                    };
                    let skip = match params.get(index + 1).copied() {
                        Some(5) => 2,
                        Some(2) => 4,
                        _ => 0,
                    };
                    if let Some(colour) = colour {
                        if target {
                            self.style.fg = colour;
                        } else {
                            self.style.bg = colour;
                        }
                    }
                    index += skip;
                }
                _ => {}
            }
            index += 1;
        }
    }

    /// One visible line as runs of one style, trailing blanks dropped.
    pub fn line_runs(&self, line: usize) -> Vec<(String, Style)> {
        let Some(cells) = self.cells.get(line * self.cols..(line + 1) * self.cols) else { return Vec::new() };
        let end = cells.iter().rposition(|cell| cell.ch != ' ' || !cell.style.is_plain()).map(|index| index + 1).unwrap_or(0);
        let mut runs: Vec<(String, Style)> = Vec::new();
        for cell in &cells[..end] {
            match runs.last_mut() {
                Some((text, style)) if *style == cell.style => text.push(cell.ch),
                _ => runs.push((cell.ch.to_string(), cell.style)),
            }
        }
        runs
    }

    /// The visible window's lines, oldest first, with the scroll position
    /// applied: `0` is the bottom of the screen.
    pub fn visible(&self, scroll_back: usize) -> Vec<Vec<(String, Style)>> {
        let total = self.scrollback.len() + self.rows;
        let end = total.saturating_sub(scroll_back.min(self.scrollback.len()));
        let start = end.saturating_sub(self.rows);
        (start..end)
            .map(|line| match line.checked_sub(self.scrollback.len()) {
                Some(row) => self.line_runs(row),
                None => {
                    let cells = &self.scrollback[line];
                    let end = cells.iter().rposition(|cell| cell.ch != ' ' || !cell.style.is_plain()).map(|index| index + 1).unwrap_or(0);
                    let mut runs: Vec<(String, Style)> = Vec::new();
                    for cell in &cells[..end] {
                        match runs.last_mut() {
                            Some((text, style)) if *style == cell.style => text.push(cell.ch),
                            _ => runs.push((cell.ch.to_string(), cell.style)),
                        }
                    }
                    runs
                }
            })
            .collect()
    }
}

/// The grid a terminal of this pixel size holds, for the pty's own size.
pub fn grid_for(width: f32, height: f32, char_width: f32, line_height: f32) -> (u16, u16) {
    const MAX_COLS: u16 = 500;
    const MAX_ROWS: u16 = 300;
    let cols = (width.max(0.0) / char_width.max(1.0)).floor();
    let rows = (height.max(0.0) / line_height.max(1.0)).floor();
    let cols = cols.clamp(1.0, MAX_COLS as f32) as u16;
    let rows = rows.clamp(1.0, MAX_ROWS as f32) as u16;
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(screen: &Screen, line: usize) -> String {
        screen.line_runs(line).into_iter().map(|(text, _)| text).collect()
    }

    #[test]
    fn plain_text_lands_on_the_grid() {
        let mut screen = Screen::new(20, 4);
        screen.feed("hello");
        assert_eq!(text_of(&screen, 0), "hello");
        assert_eq!(screen.cursor(), (0, 5));
    }

    #[test]
    fn a_carriage_return_starts_the_line_again_and_a_newline_moves_down() {
        let mut screen = Screen::new(20, 4);
        screen.feed("abc\rXY");
        assert_eq!(text_of(&screen, 0), "XYc");
        screen.feed("\r\ndef");
        assert_eq!(text_of(&screen, 1), "def");
    }

    #[test]
    fn text_that_runs_past_the_last_column_wraps() {
        let mut screen = Screen::new(5, 3);
        screen.feed("abcdefg");
        assert_eq!(text_of(&screen, 0), "abcde");
        assert_eq!(text_of(&screen, 1), "fg");
        assert_eq!(screen.cursor(), (1, 2));
    }

    #[test]
    fn scrolling_keeps_the_lines_that_left_the_screen() {
        let mut screen = Screen::new(10, 2);
        screen.feed("one\r\ntwo\r\nthree");
        assert_eq!(screen.scrollback_len(), 1);
        assert_eq!(text_of(&screen, 0), "two");
        assert_eq!(text_of(&screen, 1), "three");
        let window = screen.visible(1);
        assert_eq!(window[0].iter().map(|(text, _)| text.clone()).collect::<String>(), "one");
    }

    #[test]
    fn a_colour_sequence_styles_only_the_text_after_it() {
        let mut screen = Screen::new(30, 2);
        screen.feed("plain \u{1b}[31mred\u{1b}[0m plain");
        let runs = screen.line_runs(0);
        assert_eq!(runs.len(), 3);
        assert!(runs[0].1.is_plain());
        assert_eq!(runs[1].1.fg, Colour::Indexed(1));
        assert_eq!(runs[1].0, "red");
        assert!(runs[2].1.is_plain());
    }

    #[test]
    fn bright_colours_bold_and_inverse_are_understood() {
        let mut screen = Screen::new(30, 2);
        screen.feed("\u{1b}[1;92mhi\u{1b}[7m!\u{1b}[22;27m?");
        let runs = screen.line_runs(0);
        assert!(runs[0].1.bold);
        assert_eq!(runs[0].1.fg, Colour::Indexed(10));
        assert!(runs[1].1.inverse);
        assert!(!runs[2].1.bold && !runs[2].1.inverse);
    }

    #[test]
    fn an_extended_colour_is_kept_as_it_arrives() {
        let mut screen = Screen::new(10, 2);
        screen.feed("\u{1b}[38;5;196mX");
        assert_eq!(screen.line_runs(0)[0].1.fg, Colour::Indexed(196));
        let mut screen = Screen::new(10, 2);
        screen.feed("\u{1b}[38;2;61;220;151mX");
        assert_eq!(screen.line_runs(0)[0].1.fg, Colour::Rgb(61, 220, 151));
    }

    #[test]
    fn erase_in_line_and_display_clear_what_they_should() {
        let mut screen = Screen::new(10, 3);
        screen.feed("abcdef\u{1b}[3D\u{1b}[K");
        assert_eq!(text_of(&screen, 0), "abc");
        screen.feed("\u{1b}[2J");
        assert_eq!(text_of(&screen, 0), "");
        assert_eq!(screen.cursor(), (0, 0));
    }

    #[test]
    fn the_cursor_can_be_moved_and_hidden() {
        let mut screen = Screen::new(20, 5);
        screen.feed("\u{1b}[3;4HX");
        assert_eq!(text_of(&screen, 2), "   X");
        assert_eq!(screen.cursor(), (2, 4));
        screen.feed("\u{1b}[?25l");
        assert!(!screen.cursor_visible());
        screen.feed("\u{1b}[?25h");
        assert!(screen.cursor_visible());
    }

    #[test]
    fn a_title_sequence_is_swallowed_and_never_drawn() {
        let mut screen = Screen::new(20, 2);
        screen.feed("\u{1b}]0;powershell\u{7}ok");
        assert_eq!(text_of(&screen, 0), "ok");
    }

    #[test]
    fn backspace_and_tab_move_the_cursor_without_drawing() {
        let mut screen = Screen::new(20, 2);
        // Backspace moves the caret; it does not erase, as a terminal does not.
        screen.feed("ab\u{8}\u{8}Z\tT");
        assert_eq!(text_of(&screen, 0), "Zb      T");
    }

    #[test]
    fn a_resize_keeps_the_text_that_still_fits() {
        let mut screen = Screen::new(10, 3);
        screen.feed("one\r\ntwo\r\nthree");
        screen.resize(10, 2);
        assert_eq!(screen.rows, 2);
        assert_eq!(text_of(&screen, 1), "two", "a shrink keeps the rows that still fit");
        assert_eq!(text_of(&screen, 0), "one");
        screen.resize(4, 2);
        assert_eq!(text_of(&screen, 1), "two", "a narrow grid cuts each row, not the rows");
    }

    #[test]
    fn the_pty_size_comes_from_the_pixels_and_is_never_zero() {
        assert_eq!(grid_for(800.0, 400.0, 8.0, 16.0), (100, 25));
        assert_eq!(grid_for(0.0, 0.0, 8.0, 16.0), (1, 1));
        assert_eq!(grid_for(10_000.0, 10_000.0, 8.0, 16.0), (500, 300));
    }

    #[test]
    fn a_line_drops_its_trailing_blanks_but_keeps_the_style_that_made_them() {
        let mut screen = Screen::new(10, 2);
        screen.feed("\u{1b}[41mab   ");
        assert_eq!(text_of(&screen, 0), "ab   ", "a styled blank is still part of the line");
        let mut screen = Screen::new(10, 2);
        screen.feed("ab   ");
        assert_eq!(text_of(&screen, 0), "ab");
    }
}
