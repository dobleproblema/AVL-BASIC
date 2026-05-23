use crossterm::cursor::{Hide, MoveTo, MoveToColumn, Show};
use crossterm::event::{poll, read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use std::collections::{HashMap, HashSet};
#[cfg(windows)]
use std::ffi::c_void;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const RESET: &str = "\x1b[0m";
const ITALICS: &str = "\x1b[3m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const GRAY: &str = "\x1b[90m";
const TAN: &str = "\x1b[38;5;214m";
const ORCHID: &str = "\x1b[38;5;165m";
const WHEAT: &str = "\x1b[38;5;229m";
const SILVER: &str = "\x1b[38;5;248m";

const KEYWORD_STYLE: &str = "\x1b[1m\x1b[3m\x1b[97m";
const PROMPT_STYLE: &str = GREEN;
const COMMENT_STYLE: &str = GREEN;
const LINE_NUMBER_STYLE: &str = TAN;
const VARIABLE_STYLE: &str = "\x1b[1m\x1b[38;5;39m";
const NUMBER_STYLE: &str = TAN;
const CURSOR_MARKER: char = '\u{E000}';

static INPUT_HISTORY: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static CTRL_C_HANDLER: OnceLock<Result<(), String>> = OnceLock::new();
static INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);
const STRING_STYLE: &str = ORCHID;
const HEX_STYLE: &str = WHEAT;
const BIN_STYLE: &str = WHEAT;
const OTHER_STYLE: &str = SILVER;
const HEADER_STYLE: &str = GRAY;

const KEYWORDS: &[&str] = &[
    "REM",
    "CLEAR",
    "CLS",
    "DATA",
    "DIM",
    "REDIM",
    "LET",
    "PRINT",
    "MAT",
    "ROW",
    "COL",
    "BASE",
    "USING",
    "INPUT",
    "LINE",
    "RANDOMIZE",
    "ERROR",
    "GOTO",
    "IF",
    "THEN",
    "ELSE",
    "ELSEIF",
    "ENDIF",
    "FOR",
    "TO",
    "NEXT",
    "STEP",
    "RETURN",
    "GOSUB",
    "ON",
    "OFF",
    "DEF",
    "FN",
    "FNEND",
    "FNEXIT",
    "SUB",
    "SUBEND",
    "SUBEXIT",
    "CALL",
    "LOCAL",
    "READ",
    "RESTORE",
    "STOP",
    "END",
    "SAVE",
    "LOAD",
    "EDIT",
    "RENUM",
    "NEW",
    "WHILE",
    "WEND",
    "LIST",
    "RUN",
    "CONT",
    "RESUME",
    "TRON",
    "TROFF",
    "FILES",
    "CAT",
    "CD",
    "DELETE",
    "EXIT",
    "QUIT",
    "SYSTEM",
    "SWAP",
    "BEEP",
    "DEBUG",
    "MOVE",
    "MOVER",
    "PLOT",
    "PLOTR",
    "DRAW",
    "DRAWR",
    "CIRCLE",
    "CIRCLER",
    "RECTANGLE",
    "TRIANGLE",
    "INK",
    "FILL",
    "PAPER",
    "SCREEN",
    "CLG",
    "LDIR",
    "MASK",
    "DEG",
    "RAD",
    "FRAME",
    "ORIGIN",
    "SCALE",
    "PENWIDTH",
    "BIGFONT",
    "SMALLFONT",
    "LOCATE",
    "DISP",
    "GDISP",
    "XAXIS",
    "YAXIS",
    "CROSSAT",
    "GRAPH",
    "GRAPHRANGE",
    "PAUSE",
    "FCIRCLE",
    "FCIRCLER",
    "FRECTANGLE",
    "FTRIANGLE",
    "BSAVE",
    "BLOAD",
    "MODE",
    "CHAIN",
    "MERGE",
    "AFTER",
    "EVERY",
    "DI",
    "EI",
    "CANCEL",
    "SPRITE",
    "COLMODE",
    "COLCOLOR",
    "COLRESET",
    "MOUSE",
    "LEFTDOWN",
    "LEFTUP",
    "LEFTDRAG",
    "RIGHTDOWN",
    "RIGHTUP",
    "RIGHTDRAG",
    "HITTEST",
    "CLOSE",
];

const FUNCTIONS: &[&str] = &[
    "ABS",
    "INT",
    "FIX",
    "SGN",
    "LEN",
    "LBOUND",
    "FRAC",
    "SQR",
    "LOG",
    "LOG10",
    "EXP",
    "SIN",
    "COS",
    "TAN",
    "ASN",
    "ACS",
    "ATN",
    "COT",
    "RTD",
    "DTR",
    "PI",
    "MIN",
    "MAX",
    "INSTR",
    "ASC",
    "VAL",
    "LEFT$",
    "TEST",
    "RIGHT$",
    "MID$",
    "STR$",
    "CHR$",
    "BIN$",
    "HEX$",
    "DEC$",
    "UPPER$",
    "LOWER$",
    "SPACE$",
    "STRING$",
    "TRIM$",
    "UBOUND",
    "VERSION$",
    "ROUND",
    "RND",
    "TIME",
    "ERL",
    "ERR",
    "XPOS",
    "YPOS",
    "HPOS",
    "VPOS",
    "RGB",
    "RGB$",
    "INKEY$",
    "KEYDOWN",
    "SCREEN$",
    "SPRITE$",
    "WIDTH",
    "HEIGHT",
    "XMIN",
    "XMAX",
    "YMIN",
    "YMAX",
    "BORDER",
    "REMAIN",
    "HIT",
    "HITCOLOR",
    "HITSPRITE",
    "HITID",
    "ZER",
    "CON",
    "IDN",
    "DET",
    "TRN",
    "INV",
    "MOUSEX",
    "MOUSEY",
    "MOUSELEFT",
    "MOUSERIGHT",
    "MOUSEEVENT$",
    "ABSUM",
    "AMAX",
    "AMAXCOL",
    "AMAXROW",
    "AMIN",
    "AMINCOL",
    "AMINROW",
    "CNORM",
    "CNORMCOL",
    "DOT",
    "FNORM",
    "LBND",
    "MAXAB",
    "MAXABCOL",
    "MAXABROW",
    "RNORM",
    "RNORMROW",
    "SUM",
    "UBND",
];

const PRINT_FUNCTIONS: &[&str] = &["SPC", "TAB"];

const NUMERIC_CONSTANTS: &[&str] = &["INF"];

const OPERATORS: &[&str] = &["MOD", "AND", "OR", "NOT", "XOR"];

pub fn ansi_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    match std::env::var("AVL_BASIC_COLOR") {
        Ok(value) if value.eq_ignore_ascii_case("always") => true,
        Ok(value) if value.eq_ignore_ascii_case("never") => false,
        _ => io::stdout().is_terminal(),
    }
}

pub fn install_ctrl_c_handler() -> io::Result<()> {
    let result = CTRL_C_HANDLER.get_or_init(|| {
        ctrlc::set_handler(|| {
            INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
        })
        .map_err(|err| err.to_string())
    });
    match result {
        Ok(()) => Ok(()),
        Err(message) => Err(io::Error::new(io::ErrorKind::Other, message.clone())),
    }
}

pub fn take_interrupt_requested() -> bool {
    INTERRUPT_REQUESTED.swap(false, Ordering::SeqCst)
}

pub fn clear_interrupt_requested() {
    INTERRUPT_REQUESTED.store(false, Ordering::SeqCst);
}

pub fn flush_pending_input() {
    clear_interrupt_requested();
    flush_platform_input();
}

pub fn request_interrupt() {
    INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
}

#[cfg(windows)]
fn flush_platform_input() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n_std_handle: i32) -> *mut c_void;
        fn FlushConsoleInputBuffer(h_console_input: *mut c_void) -> i32;
    }

    const STD_INPUT_HANDLE: i32 = -10;
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null() || handle == (-1isize as *mut c_void) {
            return;
        }
        let _ = FlushConsoleInputBuffer(handle);
    }
}

#[cfg(not(windows))]
fn flush_platform_input() {}

pub fn request_interrupt_for_test() {
    request_interrupt();
}

pub fn interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub struct RuntimeRawModeGuard {
    active: bool,
}

impl RuntimeRawModeGuard {
    fn inactive() -> Self {
        Self { active: false }
    }
}

impl Drop for RuntimeRawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = runtime_raw::leave();
        }
    }
}

pub struct RuntimeRawModeSuspendGuard {
    active: bool,
}

impl RuntimeRawModeSuspendGuard {
    fn inactive() -> Self {
        Self { active: false }
    }
}

impl Drop for RuntimeRawModeSuspendGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = runtime_raw::resume();
        }
    }
}

pub fn enter_runtime_raw_mode() -> io::Result<RuntimeRawModeGuard> {
    runtime_raw::enter()
}

pub fn suspend_runtime_raw_mode() -> io::Result<RuntimeRawModeSuspendGuard> {
    runtime_raw::suspend()
}

pub fn read_runtime_key_code() -> Option<u8> {
    runtime_raw::read_key_code()
}

pub fn prompt_text(ansi: bool, plain: &str) -> String {
    if ansi {
        format!("{PROMPT_STYLE}{plain}{RESET}")
    } else {
        plain.to_string()
    }
}

pub fn error_text(ansi: bool, text: &str) -> String {
    if ansi {
        format!("{ITALICS}{RED}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn trace_text(ansi: bool, line: i32) -> String {
    let text = format!("[{line}]");
    if ansi {
        format!("{LINE_NUMBER_STYLE}{text}{RESET}")
    } else {
        text
    }
}

pub fn normalize_code(code: &str) -> String {
    let (main, comment) = split_single_quote_comment(code);
    let mut result = normalize_main_code(main.trim_end());
    result = add_bas_extension_to_leading_file_command(&result);
    result = format_colon_separators(&result);
    if let Some((spaces, comment)) = comment {
        result.push_str(&" ".repeat(spaces));
        result.push('\'');
        result.push_str(comment);
    }
    result
}

pub fn syntax_highlight(line: &str, ansi: bool) -> String {
    syntax_highlight_with_cases(line, ansi, None)
}

pub fn syntax_highlight_with_cases(
    line: &str,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
) -> String {
    let mut line = normalize_code(line);
    if let Some(cases) = cases {
        line = apply_identifier_case_for_display(&line, cases);
    }
    if !ansi {
        return line;
    }
    let (main, comment) = split_single_quote_comment(&line);
    let mut out = String::new();
    let mut rest = main;
    if let Some((line_no, after)) = split_line_number(rest) {
        out.push_str(LINE_NUMBER_STYLE);
        out.push_str(line_no);
        out.push_str(RESET);
        rest = after;
    }
    out.push_str(&highlight_main(rest));
    if let Some((spaces, comment)) = comment {
        out.push_str(&" ".repeat(spaces));
        out.push('\'');
        out.push_str(COMMENT_STYLE);
        out.push_str(comment);
        out.push_str(RESET);
    }
    out
}

pub fn syntax_highlight_raw_with_cases(
    line: &str,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
) -> String {
    let mut line = line.to_string();
    if let Some(cases) = cases {
        line = apply_identifier_case_for_display(&line, cases);
    }
    if !ansi {
        return line;
    }
    let (main, comment) = split_single_quote_comment(&line);
    let mut out = String::new();
    let mut rest = main;
    if let Some((line_no, after)) = split_line_number(rest) {
        out.push_str(LINE_NUMBER_STYLE);
        out.push_str(line_no);
        out.push_str(RESET);
        rest = after;
    }
    out.push_str(&highlight_main(rest));
    if let Some((spaces, comment)) = comment {
        out.push_str(&" ".repeat(spaces));
        out.push('\'');
        out.push_str(COMMENT_STYLE);
        out.push_str(comment);
        out.push_str(RESET);
    }
    out
}

pub fn read_highlighted_line(
    prompt: &str,
    prefill: &str,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
) -> io::Result<String> {
    read_highlighted_line_with_idle(prompt, prefill, ansi, cases, || Ok(()))
}

pub fn read_highlighted_line_with_idle<F>(
    prompt: &str,
    prefill: &str,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    mut idle: F,
) -> io::Result<String>
where
    F: FnMut() -> io::Result<()>,
{
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        print!("{prompt}{prefill}");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stdin closed"));
        }
        return Ok(if line.trim_end_matches(&['\r', '\n'][..]).is_empty() {
            prefill.to_string()
        } else {
            line.trim_end_matches(&['\r', '\n'][..]).to_string()
        });
    }

    let mut buffer: Vec<char> = prefill.chars().collect();
    let mut cursor = buffer.len();
    let use_history = prefill.is_empty();
    let mut history = HistoryNavigation::default();
    enable_raw_mode()?;
    redraw_input_line(prompt, &buffer, cursor, ansi, cases)?;
    loop {
        if !poll(Duration::from_millis(30))? {
            idle()?;
            continue;
        }
        match read()? {
            Event::Key(event) => match event.code {
                _ if event.kind == KeyEventKind::Release => {}
                KeyCode::Enter => {
                    disable_raw_mode()?;
                    println!();
                    let result: String = buffer.iter().collect();
                    if use_history {
                        remember_history(&result);
                    }
                    return Ok(result);
                }
                KeyCode::Esc => {
                    if prefill.is_empty() {
                        buffer.clear();
                        cursor = 0;
                        history.reset();
                    } else {
                        buffer = prefill.chars().collect();
                        cursor = buffer.len();
                        redraw_input_line(prompt, &buffer, cursor, ansi, cases)?;
                        disable_raw_mode()?;
                        println!();
                        return Ok(prefill.to_string());
                    }
                }
                KeyCode::Char(ch) if is_ctrl_c_key(ch, event.modifiers) => {
                    disable_raw_mode()?;
                    println!();
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "Ctrl-C"));
                }
                KeyCode::Char(ch) => {
                    buffer.insert(cursor, ch);
                    cursor += 1;
                    history.reset();
                }
                KeyCode::Backspace => {
                    if cursor > 0 {
                        cursor -= 1;
                        buffer.remove(cursor);
                        history.reset();
                    }
                }
                KeyCode::Delete => {
                    if cursor < buffer.len() {
                        buffer.remove(cursor);
                        history.reset();
                    }
                }
                KeyCode::Left => cursor = cursor.saturating_sub(1),
                KeyCode::Right => cursor = (cursor + 1).min(buffer.len()),
                KeyCode::Home => cursor = 0,
                KeyCode::End => cursor = buffer.len(),
                KeyCode::Up if use_history => {
                    if let Some(next) = history.previous(&history_snapshot(), &buffer) {
                        buffer = next;
                        cursor = buffer.len();
                    }
                }
                KeyCode::Down if use_history => {
                    if let Some(next) = history.next(&history_snapshot()) {
                        buffer = next;
                        cursor = buffer.len();
                    }
                }
                KeyCode::Tab => {}
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
        redraw_input_line(prompt, &buffer, cursor, ansi, cases)?;
    }
}

pub enum FullscreenEditOutcome {
    Apply(Vec<String>),
    Cancel,
}

pub fn edit_fullscreen<F>(
    initial_lines: &[String],
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    mut validate: F,
) -> io::Result<FullscreenEditOutcome>
where
    F: FnMut(&[String]) -> Result<(), String>,
{
    if !interactive_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "full-screen editor requires an interactive terminal",
        ));
    }

    let _guard = FullscreenEditorGuard::enter()?;
    let mut editor = BasicEditor::new(initial_lines);
    let mut status = BasicEditor::default_help();
    render_fullscreen_editor(&mut editor, ansi, cases, &status)?;

    loop {
        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Release {
                    continue;
                }
                match event.code {
                    KeyCode::F(12) => {
                        let lines = editor.lines_as_strings();
                        match validate(&lines) {
                            Ok(()) => return Ok(FullscreenEditOutcome::Apply(lines)),
                            Err(message) => {
                                status = format!("Apply failed: {message}");
                            }
                        }
                    }
                    KeyCode::F(3) => {
                        status = if editor.undo() {
                            String::from("Undone")
                        } else {
                            String::from("Nothing to undo")
                        };
                    }
                    KeyCode::F(4) => {
                        status = if editor.redo() {
                            String::from("Redone")
                        } else {
                            String::from("Nothing to redo")
                        };
                    }
                    KeyCode::F(5) => {
                        status = if editor.copy_selection() {
                            String::from("Copied")
                        } else {
                            String::from("No selection")
                        };
                    }
                    KeyCode::F(6) => {
                        status = if editor.paste_clipboard() {
                            String::from("Pasted")
                        } else {
                            String::from("Clipboard empty")
                        };
                    }
                    KeyCode::F(8) => {
                        status = run_editor_replace(&mut editor, ansi, cases)?;
                    }
                    KeyCode::F(7) => {
                        let initial = editor.last_find.clone();
                        let Some(query) =
                            read_editor_prompt(&mut editor, ansi, cases, "Find: ", &initial)?
                        else {
                            status = editor.default_status();
                            render_fullscreen_editor(&mut editor, ansi, cases, &status)?;
                            continue;
                        };
                        if query.is_empty() {
                            status = String::from("Find text empty");
                            render_fullscreen_editor(&mut editor, ansi, cases, &status)?;
                            continue;
                        }

                        editor.last_find = query.clone();
                        status = match editor.find_next(&query) {
                            Some(EditorFindResult::Found) => String::from("Found"),
                            Some(EditorFindResult::Wrapped) => String::from("Found (wrapped)"),
                            None => String::from("Not found"),
                        };
                    }
                    KeyCode::F(9) => {
                        status = match editor.renumber_visible_lines() {
                            Ok(()) => String::from("Renumbered"),
                            Err(message) => format!("Renum failed: {message}"),
                        };
                    }
                    KeyCode::Esc => return Ok(FullscreenEditOutcome::Cancel),
                    KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                        editor.insert_char(ch);
                        status = editor.default_status();
                    }
                    KeyCode::Enter => {
                        editor.insert_newline();
                        status = editor.default_status();
                    }
                    KeyCode::Tab => {
                        editor.insert_text("  ");
                        status = editor.default_status();
                    }
                    KeyCode::Backspace => {
                        editor.backspace();
                        status = editor.default_status();
                    }
                    KeyCode::Delete => {
                        editor.delete();
                        status = editor.default_status();
                    }
                    KeyCode::Left if event.modifiers.contains(KeyModifiers::SHIFT) => {
                        editor.select_left();
                        status = editor.default_status();
                    }
                    KeyCode::Right if event.modifiers.contains(KeyModifiers::SHIFT) => {
                        editor.select_right();
                        status = editor.default_status();
                    }
                    KeyCode::Up if event.modifiers.contains(KeyModifiers::SHIFT) => {
                        editor.select_up();
                        status = editor.default_status();
                    }
                    KeyCode::Down if event.modifiers.contains(KeyModifiers::SHIFT) => {
                        editor.select_down();
                        status = editor.default_status();
                    }
                    KeyCode::Left => {
                        editor.move_left();
                        status = editor.default_status();
                    }
                    KeyCode::Right => {
                        editor.move_right();
                        status = editor.default_status();
                    }
                    KeyCode::Up => {
                        editor.move_up();
                        status = editor.default_status();
                    }
                    KeyCode::Down => {
                        editor.move_down();
                        status = editor.default_status();
                    }
                    KeyCode::Home if event.modifiers.contains(KeyModifiers::CONTROL) => {
                        editor.move_document_start();
                        status = editor.default_status();
                    }
                    KeyCode::End if event.modifiers.contains(KeyModifiers::CONTROL) => {
                        editor.move_document_end();
                        status = editor.default_status();
                    }
                    KeyCode::Home => {
                        editor.move_home();
                        status = editor.default_status();
                    }
                    KeyCode::End => {
                        editor.move_end();
                        status = editor.default_status();
                    }
                    KeyCode::PageUp => {
                        editor.page_up();
                        status = editor.default_status();
                    }
                    KeyCode::PageDown => {
                        editor.page_down();
                        status = editor.default_status();
                    }
                    _ => {}
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
        render_fullscreen_editor(&mut editor, ansi, cases, &status)?;
    }
}

struct FullscreenEditorGuard;

impl FullscreenEditorGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(err) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(err);
        }
        Ok(Self)
    }
}

impl Drop for FullscreenEditorGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

const MAX_UNDO_STEPS: usize = 200;
const SELECTION_STYLE: &str = "\x1b[7m";
const SELECTION_END_STYLE: &str = "\x1b[27m";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EditorPosition {
    line: usize,
    col: usize,
}

#[derive(Clone)]
struct EditorSnapshot {
    lines: Vec<Vec<char>>,
    cursor_line: usize,
    cursor_col: usize,
    top_line: usize,
    left_col: usize,
    dirty: bool,
    selection_anchor: Option<EditorPosition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorFindResult {
    Found,
    Wrapped,
}

struct BasicEditor {
    lines: Vec<Vec<char>>,
    cursor_line: usize,
    cursor_col: usize,
    top_line: usize,
    left_col: usize,
    page_rows: usize,
    dirty: bool,
    selection_anchor: Option<EditorPosition>,
    clipboard: String,
    last_find: String,
    last_replace: String,
    undo_stack: Vec<EditorSnapshot>,
    redo_stack: Vec<EditorSnapshot>,
}

impl BasicEditor {
    fn new(initial_lines: &[String]) -> Self {
        let mut lines: Vec<Vec<char>> = initial_lines
            .iter()
            .map(|line| line.chars().collect::<Vec<_>>())
            .collect();
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        Self {
            lines,
            cursor_line: 0,
            cursor_col: 0,
            top_line: 0,
            left_col: 0,
            page_rows: 1,
            dirty: false,
            selection_anchor: None,
            clipboard: String::new(),
            last_find: String::new(),
            last_replace: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    fn lines_as_strings(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| line.iter().collect())
            .collect()
    }

    fn default_help() -> String {
        String::from(
            "F12 Apply Esc Cancel F3/F4 Undo/Redo F5/F6 Copy/Paste F7/F8 Find/Replace F9 Renum",
        )
    }

    fn default_status(&self) -> String {
        Self::default_help()
    }

    fn current_line_len(&self) -> usize {
        self.lines
            .get(self.cursor_line)
            .map_or(0, |line| line.len())
    }

    fn current_line_mut(&mut self) -> &mut Vec<char> {
        &mut self.lines[self.cursor_line]
    }

    fn position(&self) -> EditorPosition {
        EditorPosition {
            line: self.cursor_line,
            col: self.cursor_col,
        }
    }

    fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            lines: self.lines.clone(),
            cursor_line: self.cursor_line,
            cursor_col: self.cursor_col,
            top_line: self.top_line,
            left_col: self.left_col,
            dirty: self.dirty,
            selection_anchor: self.selection_anchor,
        }
    }

    fn restore_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.lines = snapshot.lines;
        self.cursor_line = snapshot.cursor_line.min(self.lines.len().saturating_sub(1));
        self.cursor_col = snapshot.cursor_col.min(self.current_line_len());
        self.top_line = snapshot.top_line.min(self.lines.len().saturating_sub(1));
        self.left_col = snapshot.left_col;
        self.dirty = snapshot.dirty;
        self.selection_anchor = snapshot.selection_anchor;
    }

    fn push_undo_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > MAX_UNDO_STEPS {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn record_undo(&mut self) {
        self.push_undo_snapshot(self.snapshot());
    }

    fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    fn selection_range(&self) -> Option<(EditorPosition, EditorPosition)> {
        let anchor = self.selection_anchor?;
        let cursor = self.position();
        if anchor == cursor {
            None
        } else if anchor < cursor {
            Some((anchor, cursor))
        } else {
            Some((cursor, anchor))
        }
    }

    fn selection_columns_for_line(&self, line_index: usize) -> Option<(usize, usize)> {
        let (start, end) = self.selection_range()?;
        if line_index < start.line || line_index > end.line {
            return None;
        }

        let line_len = self.lines.get(line_index).map_or(0, Vec::len);
        let selection_start = if line_index == start.line {
            start.col.min(line_len)
        } else {
            0
        };
        let selection_end = if line_index == end.line {
            end.col.min(line_len)
        } else {
            line_len
        };

        (selection_start < selection_end).then_some((selection_start, selection_end))
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        if start.line == end.line {
            return Some(
                self.lines[start.line][start.col..end.col]
                    .iter()
                    .copied()
                    .collect(),
            );
        }

        let mut text = String::new();
        text.extend(self.lines[start.line][start.col..].iter().copied());
        text.push('\n');
        for line in (start.line + 1)..end.line {
            text.extend(self.lines[line].iter().copied());
            text.push('\n');
        }
        text.extend(self.lines[end.line][..end.col].iter().copied());
        Some(text)
    }

    fn selected_text_matches(&self, query: &[char]) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        let text: Vec<char> = text.chars().collect();
        text.len() == query.len()
            && text
                .iter()
                .zip(query.iter())
                .all(|(left, right)| chars_equal_ignore_ascii_case(*left, *right))
    }

    fn set_selection_range(&mut self, start: EditorPosition, end: EditorPosition) {
        if start == end {
            self.cursor_line = end.line.min(self.lines.len().saturating_sub(1));
            self.cursor_col = end.col.min(self.current_line_len());
            self.clear_selection();
            return;
        }

        self.selection_anchor = Some(start);
        self.cursor_line = end.line.min(self.lines.len().saturating_sub(1));
        self.cursor_col = end.col.min(self.current_line_len());
    }

    fn find_next(&mut self, query: &str) -> Option<EditorFindResult> {
        let query: Vec<char> = query.chars().collect();
        if query.is_empty() {
            return None;
        }

        let start = self
            .selection_range()
            .map(|(_, end)| end)
            .unwrap_or_else(|| self.position());
        if let Some(match_start) = self.find_match_from(&query, start) {
            self.select_match(match_start, query.len());
            return Some(EditorFindResult::Found);
        }

        if start.line != 0 || start.col != 0 {
            let document_start = EditorPosition { line: 0, col: 0 };
            if let Some(match_start) = self.find_match_from(&query, document_start) {
                self.select_match(match_start, query.len());
                return Some(EditorFindResult::Wrapped);
            }
        }

        None
    }

    fn find_next_without_wrap(&mut self, query: &str) -> bool {
        let query: Vec<char> = query.chars().collect();
        if query.is_empty() {
            return false;
        }

        let start = self
            .selection_range()
            .map(|(_, end)| end)
            .unwrap_or_else(|| self.position());
        let Some(match_start) = self.find_match_from(&query, start) else {
            return false;
        };
        self.select_match(match_start, query.len());
        true
    }

    fn find_first(&mut self, query: &str) -> bool {
        let query: Vec<char> = query.chars().collect();
        if query.is_empty() {
            return false;
        }

        let Some(match_start) = self.find_match_from(&query, EditorPosition { line: 0, col: 0 })
        else {
            return false;
        };
        self.select_match(match_start, query.len());
        true
    }

    fn find_match_from(&self, query: &[char], start: EditorPosition) -> Option<EditorPosition> {
        for line_index in start.line..self.lines.len() {
            let line = &self.lines[line_index];
            if query.len() > line.len() {
                continue;
            }

            let start_col = if line_index == start.line {
                start.col.min(line.len())
            } else {
                0
            };
            if start_col + query.len() > line.len() {
                continue;
            }

            for col in start_col..=(line.len() - query.len()) {
                if line_matches_at_ignore_ascii_case(line, col, query) {
                    return Some(EditorPosition {
                        line: line_index,
                        col,
                    });
                }
            }
        }
        None
    }

    fn select_match(&mut self, start: EditorPosition, len: usize) {
        let end = EditorPosition {
            line: start.line,
            col: start.col + len,
        };
        self.set_selection_range(start, end);
    }

    fn replace_selected_match(&mut self, query: &str, replacement: &str) -> bool {
        let query_chars: Vec<char> = query.chars().collect();
        if query_chars.is_empty() || !self.selected_text_matches(&query_chars) {
            return false;
        }

        self.record_undo();
        self.replace_selected_without_history(replacement)
    }

    fn replace_all_from_selection_to_end(&mut self, query: &str, replacement: &str) -> usize {
        let query_chars: Vec<char> = query.chars().collect();
        if query_chars.is_empty() {
            return 0;
        }

        if !self.selected_text_matches(&query_chars) {
            let start = self.position();
            let Some(match_start) = self.find_match_from(&query_chars, start) else {
                return 0;
            };
            self.select_match(match_start, query_chars.len());
        }

        let snapshot = self.snapshot();
        let mut count = 0usize;
        loop {
            if !self.selected_text_matches(&query_chars) {
                break;
            }
            self.replace_selected_without_history(replacement);
            count += 1;

            let next_start = self.position();
            let Some(match_start) = self.find_match_from(&query_chars, next_start) else {
                break;
            };
            self.select_match(match_start, query_chars.len());
        }

        if count > 0 {
            self.push_undo_snapshot(snapshot);
        }
        count
    }

    fn delete_selection_without_history(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };

        if start.line == end.line {
            self.lines[start.line].drain(start.col..end.col);
        } else {
            let tail: Vec<char> = self.lines[end.line][end.col..].to_vec();
            self.lines[start.line].truncate(start.col);
            self.lines[start.line].extend(tail);
            self.lines.drain((start.line + 1)..=end.line);
        }

        self.cursor_line = start.line;
        self.cursor_col = start.col;
        self.clear_selection();
        self.dirty = true;
        true
    }

    fn replace_selected_without_history(&mut self, replacement: &str) -> bool {
        if !self.delete_selection_without_history() {
            return false;
        }
        self.insert_text_without_history(replacement);
        self.clear_selection();
        self.dirty = true;
        true
    }

    fn insert_text_without_history(&mut self, text: &str) {
        let parts: Vec<&str> = text.split('\n').collect();
        let col = self.cursor_col.min(self.current_line_len());
        if parts.len() == 1 {
            let chars: Vec<char> = parts[0].chars().collect();
            let inserted = chars.len();
            self.current_line_mut().splice(col..col, chars);
            self.cursor_col = col + inserted;
            return;
        }

        let tail = self.current_line_mut().split_off(col);
        self.current_line_mut().extend(parts[0].chars());

        let mut insert_at = self.cursor_line + 1;
        for part in &parts[1..parts.len() - 1] {
            self.lines.insert(insert_at, part.chars().collect());
            insert_at += 1;
        }

        let mut last_line: Vec<char> = parts.last().unwrap_or(&"").chars().collect();
        let cursor_col = last_line.len();
        last_line.extend(tail);
        self.lines.insert(insert_at, last_line);
        self.cursor_line = insert_at;
        self.cursor_col = cursor_col;
    }

    fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.record_undo();
        self.delete_selection_without_history();
        self.insert_text_without_history(text);
        self.clear_selection();
        self.dirty = true;
    }

    fn insert_char(&mut self, ch: char) {
        let mut text = String::new();
        text.push(ch);
        self.insert_text(&text);
    }

    fn insert_newline(&mut self) {
        self.record_undo();
        self.delete_selection_without_history();
        let col = self.cursor_col.min(self.current_line_len());
        let tail = self.current_line_mut().split_off(col);
        self.cursor_line += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_line, tail);
        self.clear_selection();
        self.dirty = true;
    }

    fn backspace(&mut self) {
        if self.selection_range().is_some() {
            self.record_undo();
            self.delete_selection_without_history();
            return;
        }
        if self.cursor_col > 0 {
            self.record_undo();
            self.cursor_col -= 1;
            let col = self.cursor_col;
            self.current_line_mut().remove(col);
            self.dirty = true;
        } else if self.cursor_line > 0 {
            self.record_undo();
            let removed = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
            self.lines[self.cursor_line].extend(removed);
            self.dirty = true;
        }
    }

    fn delete(&mut self) {
        if self.selection_range().is_some() {
            self.record_undo();
            self.delete_selection_without_history();
            return;
        }
        let len = self.current_line_len();
        if self.cursor_col < len {
            self.record_undo();
            let col = self.cursor_col;
            self.current_line_mut().remove(col);
            self.dirty = true;
        } else if self.cursor_line + 1 < self.lines.len() {
            self.record_undo();
            let next = self.lines.remove(self.cursor_line + 1);
            self.current_line_mut().extend(next);
            self.dirty = true;
        }
    }

    fn copy_selection(&mut self) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        self.clipboard = text;
        true
    }

    fn paste_clipboard(&mut self) -> bool {
        if self.clipboard.is_empty() {
            return false;
        }
        let text = self.clipboard.clone();
        self.insert_text(&text);
        true
    }

    fn renumber_visible_lines(&mut self) -> Result<(), &'static str> {
        let numbered = collect_editor_line_numbers(&self.lines)?;
        if numbered.is_empty() {
            return Err("no program lines");
        }

        let mut seen = HashSet::new();
        for line in &numbered {
            if !seen.insert(line.old_number) {
                return Err("duplicate line number");
            }
        }

        let start = numbered
            .iter()
            .map(|line| line.old_number)
            .min()
            .ok_or("no program lines")?;
        let step = infer_editor_renum_step(numbered.iter().map(|line| line.old_number));

        let mut mapping = HashMap::new();
        let mut next = start;
        for line in &numbered {
            mapping.insert(line.old_number, next);
            next = next.checked_add(step).ok_or("line number overflow")?;
        }

        self.record_undo();
        for line in numbered {
            let new_number = mapping
                .get(&line.old_number)
                .copied()
                .ok_or("internal renum error")?;
            let chars = &self.lines[line.index];
            let prefix: String = chars[..line.number_start].iter().collect();
            let code: String = chars[line.number_end..].iter().collect();
            let code = renumber_editor_line_references(&code, &mapping);
            self.lines[line.index] = format!("{prefix}{new_number}{code}").chars().collect();
        }
        self.cursor_col = self.cursor_col.min(self.current_line_len());
        self.clear_selection();
        self.dirty = true;
        Ok(())
    }

    fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo_stack.pop() else {
            return false;
        };
        self.redo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot);
        true
    }

    fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo_stack.pop() else {
            return false;
        };
        self.undo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot);
        true
    }

    fn move_with_selection<F>(&mut self, extend_selection: bool, move_cursor: F)
    where
        F: FnOnce(&mut Self),
    {
        let before = self.position();
        if extend_selection && self.selection_anchor.is_none() {
            self.selection_anchor = Some(before);
        }
        move_cursor(self);
        if extend_selection {
            if self.selection_anchor == Some(self.position()) {
                self.clear_selection();
            }
        } else {
            self.clear_selection();
        }
    }

    fn move_left_with_selection(&mut self, extend_selection: bool) {
        self.move_with_selection(extend_selection, |editor| {
            editor.move_left_raw();
        });
    }

    fn move_left_raw(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.current_line_len();
        }
    }

    fn move_right_with_selection(&mut self, extend_selection: bool) {
        self.move_with_selection(extend_selection, |editor| {
            editor.move_right_raw();
        });
    }

    fn move_right_raw(&mut self) {
        if self.cursor_col < self.current_line_len() {
            self.cursor_col += 1;
        } else if self.cursor_line + 1 < self.lines.len() {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
    }

    fn move_up_with_selection(&mut self, extend_selection: bool) {
        self.move_with_selection(extend_selection, |editor| {
            editor.move_up_raw();
        });
    }

    fn move_up_raw(&mut self) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.cursor_col.min(self.current_line_len());
        }
    }

    fn move_down_with_selection(&mut self, extend_selection: bool) {
        self.move_with_selection(extend_selection, |editor| {
            editor.move_down_raw();
        });
    }

    fn move_down_raw(&mut self) {
        if self.cursor_line + 1 < self.lines.len() {
            self.cursor_line += 1;
            self.cursor_col = self.cursor_col.min(self.current_line_len());
        }
    }

    fn move_left(&mut self) {
        self.move_left_with_selection(false);
    }

    fn move_right(&mut self) {
        self.move_right_with_selection(false);
    }

    fn move_up(&mut self) {
        self.move_up_with_selection(false);
    }

    fn move_down(&mut self) {
        self.move_down_with_selection(false);
    }

    fn select_left(&mut self) {
        self.move_left_with_selection(true);
    }

    fn select_right(&mut self) {
        self.move_right_with_selection(true);
    }

    fn select_up(&mut self) {
        self.move_up_with_selection(true);
    }

    fn select_down(&mut self) {
        self.move_down_with_selection(true);
    }

    fn move_home(&mut self) {
        self.cursor_col = 0;
        self.clear_selection();
    }

    fn move_end(&mut self) {
        self.cursor_col = self.current_line_len();
        self.clear_selection();
    }

    fn move_document_start(&mut self) {
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.clear_selection();
    }

    fn move_document_end(&mut self) {
        self.cursor_line = self.lines.len().saturating_sub(1);
        self.cursor_col = self.current_line_len();
        self.clear_selection();
    }

    fn page_up(&mut self) {
        let rows = self.page_rows.max(1);
        self.cursor_line = self.cursor_line.saturating_sub(rows);
        self.cursor_col = self.cursor_col.min(self.current_line_len());
        self.clear_selection();
    }

    fn page_down(&mut self) {
        let rows = self.page_rows.max(1);
        self.cursor_line = (self.cursor_line + rows).min(self.lines.len().saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.current_line_len());
        self.clear_selection();
    }

    fn ensure_cursor_visible(&mut self, cols: usize, rows: usize) {
        self.page_rows = rows.max(1);
        if self.cursor_line < self.top_line {
            self.top_line = self.cursor_line;
        } else if self.cursor_line >= self.top_line + rows {
            self.top_line = self.cursor_line + 1 - rows;
        }

        if self.cursor_col < self.left_col {
            self.left_col = self.cursor_col;
        } else if cols > 0 && self.cursor_col >= self.left_col + cols {
            self.left_col = self.cursor_col + 1 - cols;
        }
    }
}

fn chars_equal_ignore_ascii_case(left: char, right: char) -> bool {
    left.eq_ignore_ascii_case(&right)
}

fn line_matches_at_ignore_ascii_case(line: &[char], start: usize, query: &[char]) -> bool {
    query
        .iter()
        .enumerate()
        .all(|(offset, query_ch)| chars_equal_ignore_ascii_case(line[start + offset], *query_ch))
}

struct EditorNumberedLine {
    index: usize,
    old_number: i32,
    number_start: usize,
    number_end: usize,
}

fn collect_editor_line_numbers(
    lines: &[Vec<char>],
) -> Result<Vec<EditorNumberedLine>, &'static str> {
    let mut numbered = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.iter().all(|ch| ch.is_whitespace()) {
            continue;
        }

        let mut number_start = 0usize;
        while number_start < line.len() && line[number_start].is_whitespace() {
            number_start += 1;
        }

        let mut number_end = number_start;
        while number_end < line.len() && line[number_end].is_ascii_digit() {
            number_end += 1;
        }
        if number_end == number_start {
            return Err("invalid line");
        }
        if number_end < line.len() && !line[number_end].is_whitespace() {
            return Err("invalid line");
        }

        let raw: String = line[number_start..number_end].iter().collect();
        let old_number = raw.parse::<i32>().map_err(|_| "invalid line number")?;
        if old_number <= 0 {
            return Err("invalid line number");
        }

        numbered.push(EditorNumberedLine {
            index,
            old_number,
            number_start,
            number_end,
        });
    }
    Ok(numbered)
}

fn infer_editor_renum_step(numbers: impl Iterator<Item = i32>) -> i32 {
    let mut numbers: Vec<i32> = numbers.collect();
    numbers.sort_unstable();
    numbers.dedup();

    let mut counts: HashMap<i32, usize> = HashMap::new();
    for pair in numbers.windows(2) {
        let step = pair[1] - pair[0];
        if step > 0 {
            *counts.entry(step).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .max_by(|(step_a, count_a), (step_b, count_b)| {
            count_a.cmp(count_b).then_with(|| step_b.cmp(step_a))
        })
        .map(|(step, _)| step)
        .unwrap_or(10)
}

fn renumber_editor_line_references(code: &str, mapping: &HashMap<i32, i32>) -> String {
    let chars: Vec<char> = code.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut in_string = false;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            i += 1;
            continue;
        }
        if !in_string && ch == '\'' {
            out.extend(chars[i..].iter());
            break;
        }
        if !in_string && is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            let upper = ident.to_ascii_uppercase();
            out.push_str(&ident);
            if matches!(
                upper.as_str(),
                "GOTO" | "GOSUB" | "THEN" | "ELSE" | "RESTORE" | "RESUME"
            ) {
                i = copy_renumbered_editor_line_list(&chars, i, &mut out, mapping);
            }
            continue;
        }
        out.push(ch);
        i += 1;
    }

    out
}

fn copy_renumbered_editor_line_list(
    chars: &[char],
    mut i: usize,
    out: &mut String,
    mapping: &HashMap<i32, i32>,
) -> usize {
    loop {
        while i < chars.len() && chars[i].is_whitespace() {
            out.push(chars[i]);
            i += 1;
        }

        let number_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if number_start == i {
            return i;
        }

        let raw: String = chars[number_start..i].iter().collect();
        if let Ok(old) = raw.parse::<i32>() {
            if let Some(new_number) = mapping.get(&old) {
                out.push_str(&new_number.to_string());
            } else {
                out.push_str(&raw);
            }
        } else {
            out.push_str(&raw);
        }

        let mut probe = i;
        while probe < chars.len() && chars[probe].is_whitespace() {
            probe += 1;
        }
        if probe < chars.len() && chars[probe] == ',' {
            out.extend(chars[i..=probe].iter());
            i = probe + 1;
            continue;
        }
        return i;
    }
}

fn run_editor_replace(
    editor: &mut BasicEditor,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
) -> io::Result<String> {
    let find_initial = editor.last_find.clone();
    let Some(query) = read_editor_prompt(editor, ansi, cases, "Find: ", &find_initial)? else {
        return Ok(editor.default_status());
    };
    if query.is_empty() {
        return Ok(String::from("Find text empty"));
    }

    let replace_initial = editor.last_replace.clone();
    let Some(replacement) = read_editor_prompt(editor, ansi, cases, "Replace: ", &replace_initial)?
    else {
        return Ok(editor.default_status());
    };

    editor.last_find = query.clone();
    editor.last_replace = replacement.clone();
    if !editor.find_first(&query) {
        return Ok(String::from("Not found"));
    }

    let mut status = editor_replace_status();
    render_fullscreen_editor(editor, ansi, cases, &status)?;

    loop {
        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Release {
                    continue;
                }
                match event.code {
                    KeyCode::Enter => {
                        if editor.replace_selected_match(&query, &replacement) {
                            if editor.find_next_without_wrap(&query) {
                                status = editor_replace_status();
                            } else {
                                return Ok(String::from("Replaced; no more matches"));
                            }
                        } else if editor.find_next_without_wrap(&query) {
                            status = editor_replace_status();
                        } else {
                            return Ok(String::from("No more matches"));
                        }
                    }
                    KeyCode::F(7) => {
                        if editor.find_next_without_wrap(&query) {
                            status = editor_replace_status();
                        } else {
                            return Ok(String::from("No more matches"));
                        }
                    }
                    KeyCode::F(8) => {
                        let count = editor.replace_all_from_selection_to_end(&query, &replacement);
                        return Ok(editor_replace_count_status(count));
                    }
                    KeyCode::Esc => return Ok(String::from("Replace stopped")),
                    _ => {}
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
        render_fullscreen_editor(editor, ansi, cases, &status)?;
    }
}

fn editor_replace_status() -> String {
    String::from("Replace: Enter Replace  F7 Skip  F8 All  Esc Done")
}

fn editor_replace_count_status(count: usize) -> String {
    if count == 1 {
        String::from("Replaced 1 occurrence")
    } else {
        format!("Replaced {count} occurrences")
    }
}

fn read_editor_prompt(
    editor: &mut BasicEditor,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    prompt: &str,
    initial: &str,
) -> io::Result<Option<String>> {
    let mut input: Vec<char> = initial.chars().collect();
    let mut cursor = input.len();
    render_fullscreen_editor_prompt(editor, ansi, cases, prompt, &input, cursor)?;

    loop {
        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Release {
                    continue;
                }
                match event.code {
                    KeyCode::Enter => return Ok(Some(input.iter().collect())),
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Char(ch) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
                        input.insert(cursor, ch);
                        cursor += 1;
                    }
                    KeyCode::Backspace => {
                        if cursor > 0 {
                            cursor -= 1;
                            input.remove(cursor);
                        }
                    }
                    KeyCode::Delete => {
                        if cursor < input.len() {
                            input.remove(cursor);
                        }
                    }
                    KeyCode::Left => {
                        cursor = cursor.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        cursor = (cursor + 1).min(input.len());
                    }
                    KeyCode::Home => {
                        cursor = 0;
                    }
                    KeyCode::End => {
                        cursor = input.len();
                    }
                    _ => {}
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
        render_fullscreen_editor_prompt(editor, ansi, cases, prompt, &input, cursor)?;
    }
}

fn render_fullscreen_editor_prompt(
    editor: &mut BasicEditor,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    prompt: &str,
    input: &[char],
    cursor: usize,
) -> io::Result<()> {
    render_fullscreen_editor(editor, ansi, cases, "")?;

    let (cols, rows) = size().unwrap_or((80, 24));
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;
    let status_row = rows.saturating_sub(1).min(u16::MAX as usize) as u16;
    let prompt_width = prompt.chars().count().min(cols);
    let input_width = cols.saturating_sub(prompt_width);
    let input_left = if input_width == 0 || cursor < input_width {
        0
    } else {
        cursor + 1 - input_width
    };
    let visible_input: String = input
        .iter()
        .skip(input_left)
        .take(input_width)
        .copied()
        .collect();
    let mut status_text = String::new();
    status_text.push_str(prompt);
    status_text.push_str(&visible_input);
    status_text = fit_plain_text(&status_text, cols);

    let cursor_x = if input_width == 0 {
        cols - 1
    } else {
        prompt_width + cursor.saturating_sub(input_left).min(input_width - 1)
    };

    let mut stdout = io::stdout();
    execute!(stdout, MoveTo(0, status_row), Clear(ClearType::CurrentLine))?;
    if ansi {
        write!(stdout, "\x1b[7m{status_text}\x1b[0m")?;
    } else {
        write!(stdout, "{status_text}")?;
    }
    execute!(stdout, MoveTo(cursor_x as u16, status_row), Show)?;
    stdout.flush()
}

fn render_fullscreen_editor(
    editor: &mut BasicEditor,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    status: &str,
) -> io::Result<()> {
    let (cols, rows) = size().unwrap_or((80, 24));
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;
    let edit_rows = rows.saturating_sub(1).max(1);
    editor.ensure_cursor_visible(cols, edit_rows);

    let mut stdout = io::stdout();
    execute!(stdout, Hide)?;
    for screen_row in 0..edit_rows {
        execute!(
            stdout,
            MoveTo(0, screen_row.min(u16::MAX as usize) as u16),
            Clear(ClearType::CurrentLine)
        )?;
        if let Some(line) = editor.lines.get(editor.top_line + screen_row) {
            let line_index = editor.top_line + screen_row;
            let visible: String = line.iter().skip(editor.left_col).take(cols).collect();
            let visible_len = visible.chars().count();
            let selection =
                editor
                    .selection_columns_for_line(line_index)
                    .and_then(|(start, end)| {
                        let visible_start = editor.left_col;
                        let visible_end = editor.left_col.saturating_add(cols);
                        if end <= visible_start || start >= visible_end {
                            return None;
                        }
                        let start = start.saturating_sub(visible_start).min(visible_len);
                        let end = end.saturating_sub(visible_start).min(visible_len);
                        (start < end).then_some((start, end))
                    });
            let rendered = syntax_highlight_raw_with_cases(&visible, ansi, cases);
            let rendered = apply_selection_to_rendered(&rendered, ansi, selection);
            write!(stdout, "{rendered}")?;
        }
    }

    let status_row = rows.saturating_sub(1).min(u16::MAX as usize) as u16;
    execute!(stdout, MoveTo(0, status_row), Clear(ClearType::CurrentLine))?;
    let status_text = editor_status_line(
        status,
        editor.dirty,
        editor.cursor_line + 1,
        editor.cursor_col + 1,
        cols,
    );
    if ansi {
        write!(stdout, "\x1b[7m{status_text:<cols$}\x1b[0m")?;
    } else {
        write!(stdout, "{status_text:<cols$}")?;
    }

    let cursor_x = editor
        .cursor_col
        .saturating_sub(editor.left_col)
        .min(cols - 1) as u16;
    let cursor_y = editor
        .cursor_line
        .saturating_sub(editor.top_line)
        .min(edit_rows - 1) as u16;
    execute!(stdout, MoveTo(cursor_x, cursor_y), Show)?;
    stdout.flush()
}

fn apply_selection_to_rendered(
    rendered: &str,
    ansi: bool,
    selection: Option<(usize, usize)>,
) -> String {
    let Some((selection_start, selection_end)) = selection else {
        return rendered.to_string();
    };
    if !ansi || selection_start >= selection_end {
        return rendered.to_string();
    }

    let mut out = String::new();
    let mut chars = rendered.chars().peekable();
    let mut plain_index = 0usize;
    let mut selecting = false;

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            let mut sequence = String::from(ch);
            while let Some(next) = chars.next() {
                sequence.push(next);
                if next == 'm' {
                    break;
                }
            }
            out.push_str(&sequence);
            if selecting && sequence == RESET {
                out.push_str(SELECTION_STYLE);
            }
            continue;
        }

        if plain_index == selection_start {
            out.push_str(SELECTION_STYLE);
            selecting = true;
        }
        if plain_index == selection_end {
            out.push_str(SELECTION_END_STYLE);
            selecting = false;
        }
        out.push(ch);
        plain_index += 1;
    }

    if selecting {
        out.push_str(SELECTION_END_STYLE);
    }
    out
}

fn fit_plain_text(text: &str, width: usize) -> String {
    let mut out: String = text.chars().take(width).collect();
    if out.chars().count() < width {
        out.push_str(&" ".repeat(width - out.chars().count()));
    }
    out
}

fn editor_status_line(status: &str, dirty: bool, line: usize, col: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let left = format!("{} {}", if dirty { "*" } else { " " }, status);
    let right = format!("Ln {line} Col {col}");
    let right_width = right.chars().count();
    if right_width >= width {
        return fit_plain_text(&right, width);
    }

    let gap = 1usize;
    let max_left = width.saturating_sub(right_width + gap);
    let left = truncate_plain_text(&left, max_left);
    let left_width = left.chars().count();
    let spaces = width.saturating_sub(left_width + right_width);
    format!("{left}{}{right}", " ".repeat(spaces))
}

fn truncate_plain_text(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

#[derive(Default)]
struct HistoryNavigation {
    index: Option<usize>,
    draft: Vec<char>,
}

impl HistoryNavigation {
    fn previous(&mut self, entries: &[String], buffer: &[char]) -> Option<Vec<char>> {
        if entries.is_empty() {
            return None;
        }
        let next_index = match self.index {
            Some(index) => index.saturating_sub(1),
            None => {
                self.draft = buffer.to_vec();
                entries.len() - 1
            }
        };
        self.index = Some(next_index);
        Some(entries[next_index].chars().collect())
    }

    fn next(&mut self, entries: &[String]) -> Option<Vec<char>> {
        let index = self.index?;
        if index + 1 < entries.len() {
            let next_index = index + 1;
            self.index = Some(next_index);
            Some(entries[next_index].chars().collect())
        } else {
            self.index = None;
            Some(self.draft.clone())
        }
    }

    fn reset(&mut self) {
        self.index = None;
        self.draft.clear();
    }
}

fn input_history() -> &'static Mutex<Vec<String>> {
    INPUT_HISTORY.get_or_init(|| Mutex::new(Vec::new()))
}

fn history_snapshot() -> Vec<String> {
    input_history()
        .lock()
        .map(|history| history.clone())
        .unwrap_or_default()
}

fn remember_history(line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(mut history) = input_history().lock() else {
        return;
    };
    if history.last().is_some_and(|last| last == line) {
        return;
    }
    history.push(line.to_string());
}

fn redraw_input_line(
    prompt: &str,
    buffer: &[char],
    cursor: usize,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
) -> io::Result<()> {
    let text: String = buffer.iter().collect();
    let rendered = syntax_highlight_with_cases(&text, ansi, cases);
    let prompt_width = visible_width(prompt);
    let cursor_col = prompt_width + normalized_cursor_position(&text, cursor);
    let mut stdout = io::stdout();
    execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    print!("{prompt}{rendered}");
    execute!(
        stdout,
        MoveToColumn(cursor_col.min(u16::MAX as usize) as u16)
    )?;
    stdout.flush()
}

fn normalized_cursor_position(text: &str, cursor: usize) -> usize {
    if cursor_after_unfinished_colon_separator(text, cursor) {
        return normalize_code(text).chars().count();
    }

    let mut marked = String::new();
    let mut inserted = false;
    for (idx, ch) in text.chars().enumerate() {
        if idx == cursor {
            marked.push(CURSOR_MARKER);
            inserted = true;
        }
        marked.push(ch);
    }
    if !inserted {
        marked.push(CURSOR_MARKER);
    }
    let normalized = normalize_code(&marked);
    normalized
        .chars()
        .position(|ch| ch == CURSOR_MARKER)
        .unwrap_or(cursor)
}

fn cursor_after_unfinished_colon_separator(text: &str, cursor: usize) -> bool {
    if cursor == 0 {
        return false;
    }
    let chars: Vec<char> = text.chars().collect();
    if cursor > chars.len() || chars[cursor - 1] != ':' {
        return false;
    }
    if chars[cursor..].iter().any(|ch| !ch.is_whitespace()) {
        return false;
    }

    let mut in_string = false;
    for ch in chars.iter().take(cursor - 1) {
        if *ch == '"' {
            in_string = !in_string;
        }
    }
    !in_string
}

fn is_ctrl_c_key(ch: char, modifiers: KeyModifiers) -> bool {
    matches!(ch, 'c' | 'C') && modifiers.contains(KeyModifiers::CONTROL)
}

fn visible_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

#[cfg(unix)]
mod runtime_raw {
    use super::{RuntimeRawModeGuard, RuntimeRawModeSuspendGuard};
    use std::io::{self, IsTerminal};
    use std::mem;
    use std::ptr;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::Duration;

    const STDIN_FD: libc::c_int = 0;

    #[derive(Default)]
    struct RawState {
        depth: usize,
        suspend_depth: usize,
        original: Option<libc::termios>,
    }

    fn state() -> &'static Mutex<RawState> {
        static STATE: OnceLock<Mutex<RawState>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(RawState::default()))
    }

    pub(super) fn enter() -> io::Result<RuntimeRawModeGuard> {
        if !io::stdin().is_terminal() {
            return Ok(RuntimeRawModeGuard::inactive());
        }
        let mut state = state()
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "terminal raw state poisoned"))?;
        if state.depth == 0 {
            let original = get_attrs()?;
            if state.suspend_depth == 0 {
                set_attrs(&runtime_attrs(original))?;
            }
            state.original = Some(original);
        } else if state.suspend_depth == 0 {
            if let Some(original) = state.original {
                set_attrs(&runtime_attrs(original))?;
            }
        }
        state.depth += 1;
        Ok(RuntimeRawModeGuard { active: true })
    }

    pub(super) fn leave() -> io::Result<()> {
        let mut state = state()
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "terminal raw state poisoned"))?;
        if state.depth == 0 {
            return Ok(());
        }
        state.depth -= 1;
        if state.depth == 0 {
            if let Some(original) = state.original {
                set_attrs(&original)?;
            }
            state.original = None;
            state.suspend_depth = 0;
        }
        Ok(())
    }

    pub(super) fn suspend() -> io::Result<RuntimeRawModeSuspendGuard> {
        let mut state = state()
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "terminal raw state poisoned"))?;
        let Some(original) = state.original else {
            return Ok(RuntimeRawModeSuspendGuard::inactive());
        };
        if state.depth == 0 {
            return Ok(RuntimeRawModeSuspendGuard::inactive());
        }
        if state.suspend_depth == 0 {
            set_attrs(&original)?;
        }
        state.suspend_depth += 1;
        Ok(RuntimeRawModeSuspendGuard { active: true })
    }

    pub(super) fn resume() -> io::Result<()> {
        let mut state = state()
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "terminal raw state poisoned"))?;
        if state.suspend_depth == 0 {
            return Ok(());
        }
        state.suspend_depth -= 1;
        if state.suspend_depth == 0 && state.depth > 0 {
            if let Some(original) = state.original {
                set_attrs(&runtime_attrs(original))?;
            }
        }
        Ok(())
    }

    pub(super) fn read_key_code() -> Option<u8> {
        if !io::stdin().is_terminal() {
            thread::sleep(Duration::from_micros(500));
            return None;
        }

        let mut ready = false;
        unsafe {
            let mut readfds: libc::fd_set = mem::zeroed();
            libc::FD_ZERO(&mut readfds);
            libc::FD_SET(STDIN_FD, &mut readfds);
            let mut timeout = libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            };
            let result = libc::select(
                STDIN_FD + 1,
                &mut readfds,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut timeout,
            );
            if result > 0 && libc::FD_ISSET(STDIN_FD, &readfds) {
                ready = true;
            }
        }

        if ready {
            let mut byte = 0u8;
            let read =
                unsafe { libc::read(STDIN_FD, (&mut byte as *mut u8).cast::<libc::c_void>(), 1) };
            if read == 1 {
                return Some(byte);
            }
        }

        thread::sleep(Duration::from_micros(500));
        None
    }

    fn get_attrs() -> io::Result<libc::termios> {
        let mut attrs = unsafe { mem::zeroed::<libc::termios>() };
        let result = unsafe { libc::tcgetattr(STDIN_FD, &mut attrs) };
        if result == 0 {
            Ok(attrs)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn set_attrs(attrs: &libc::termios) -> io::Result<()> {
        let result = unsafe { libc::tcsetattr(STDIN_FD, libc::TCSANOW, attrs) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn runtime_attrs(original: libc::termios) -> libc::termios {
        let mut attrs = original;
        attrs.c_lflag &= !(libc::ICANON | libc::ECHO);
        attrs.c_cc[libc::VMIN] = 0;
        attrs.c_cc[libc::VTIME] = 0;
        attrs
    }
}

#[cfg(not(unix))]
mod runtime_raw {
    use super::{RuntimeRawModeGuard, RuntimeRawModeSuspendGuard};
    use std::io;

    pub(super) fn enter() -> io::Result<RuntimeRawModeGuard> {
        Ok(RuntimeRawModeGuard::inactive())
    }

    pub(super) fn leave() -> io::Result<()> {
        Ok(())
    }

    pub(super) fn suspend() -> io::Result<RuntimeRawModeSuspendGuard> {
        Ok(RuntimeRawModeSuspendGuard::inactive())
    }

    pub(super) fn resume() -> io::Result<()> {
        Ok(())
    }

    pub(super) fn read_key_code() -> Option<u8> {
        None
    }
}

fn normalize_main_code(code: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            out.push(ch);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            if !out.ends_with('"') {
                out.push('"');
            }
            continue;
        }
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                i += 1;
                if i < chars.len() && matches!(chars[i], '+' | '-') {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push_str(&canonicalize_number(
                &chars[start..i].iter().collect::<String>(),
            ));
            continue;
        }
        if is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_ascii_uppercase();
            if upper == "REM" && token_boundary(&chars, start, i) {
                out.push_str("REM");
                out.extend(chars[i..].iter());
                return out;
            } else if is_known_word(&upper) && token_boundary(&chars, start, i) {
                out.push_str(&upper);
                if matches!(
                    upper.as_str(),
                    "LOAD" | "SAVE" | "RUN" | "CHAIN" | "MERGE" | "CAT" | "FILES" | "CD"
                ) {
                    let mut probe = i;
                    while probe < chars.len() && chars[probe] == ' ' {
                        probe += 1;
                    }
                    if probe < chars.len() && chars[probe] == '"' && i == probe {
                        out.push(' ');
                    }
                }
            } else if upper.starts_with("FN") && word.len() > 2 {
                out.push_str("FN");
                out.push_str(&word[2..].to_ascii_uppercase());
            } else {
                out.push_str(&word);
            }
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn format_colon_separators(source: &str) -> String {
    let (prefix, body) = split_line_number(source).unwrap_or(("", source));
    let statements = split_listing_statements(body);
    if statements.len() <= 1 {
        return source.to_string();
    }
    format!("{prefix}{}", statements.join(" : "))
}

fn split_listing_statements(code: &str) -> Vec<String> {
    let chars: Vec<char> = code.chars().collect();
    let mut statements = Vec::new();
    let mut buffer = String::new();
    let mut i = 0usize;
    let mut in_string = false;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            in_string = !in_string;
            buffer.push(ch);
            i += 1;
            continue;
        }

        if !in_string && starts_with_chars(&chars, i, "REM ") {
            push_statement(&mut statements, &buffer);
            buffer.clear();
            let rem: String = chars[i..].iter().collect();
            push_statement(&mut statements, &rem);
            return statements;
        }

        if !in_string && starts_with_chars(&chars, i, "IF ") {
            let mut prev = i;
            while prev > 0 && chars[prev - 1] == ' ' {
                prev -= 1;
            }
            if prev > 0 && chars[prev - 1] == ':' {
                prev -= 1;
                while prev > 0 && chars[prev - 1] == ' ' {
                    prev -= 1;
                }
            }
            let after_else = prev >= 4
                && chars[prev - 4..prev]
                    .iter()
                    .collect::<String>()
                    .eq_ignore_ascii_case("ELSE")
                && (prev < 5 || !chars[prev - 5].is_ascii_alphanumeric());
            if !after_else && (i == 0 || !chars[i - 1].is_ascii_alphanumeric()) {
                push_statement(&mut statements, &buffer);
                let if_block: String = chars[i..].iter().collect();
                push_statement(&mut statements, &if_block);
                return statements;
            }
        }

        if ch == ':' && !in_string {
            push_statement(&mut statements, &buffer);
            buffer.clear();
            i += 1;
            continue;
        }

        buffer.push(ch);
        i += 1;
    }

    push_statement(&mut statements, &buffer);
    statements
}

fn push_statement(statements: &mut Vec<String>, statement: &str) {
    let trimmed = statement.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }
}

fn starts_with_chars(chars: &[char], start: usize, needle: &str) -> bool {
    let needle_chars: Vec<char> = needle.chars().collect();
    chars
        .get(start..start + needle_chars.len())
        .is_some_and(|slice| slice == needle_chars.as_slice())
}

fn canonicalize_number(raw: &str) -> String {
    let Ok(value) = raw.parse::<f64>() else {
        return raw.to_string();
    };
    if !value.is_finite() {
        return raw.trim_start_matches('+').to_string();
    }
    if raw.contains('e') || raw.contains('E') {
        if value == 0.0 {
            return "0".to_string();
        }
        let scientific = format!("{value:.14e}");
        let Some((mantissa, exponent)) = scientific.split_once('e') else {
            return raw.to_string();
        };
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        let exponent = exponent.parse::<i32>().unwrap_or(0);
        return format!("{mantissa}E{exponent:+}");
    }
    let mut text = format!("{value:.14}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" || text == "+0" || text.is_empty() {
        "0".to_string()
    } else {
        text.trim_start_matches('+').to_string()
    }
}

fn highlight_main(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut after_def = false;
    let mut expect_sub_name = false;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            push_styled(
                &mut out,
                STRING_STYLE,
                &chars[start..i].iter().collect::<String>(),
            );
            continue;
        }
        if is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_ascii_uppercase();
            if expect_sub_name {
                push_styled(&mut out, KEYWORD_STYLE, &upper);
                expect_sub_name = false;
                after_def = false;
            } else if upper == "REM" && token_boundary(&chars, start, i) {
                push_styled(&mut out, KEYWORD_STYLE, "REM");
                push_styled(
                    &mut out,
                    COMMENT_STYLE,
                    &chars[i..].iter().collect::<String>(),
                );
                return out;
            } else if KEYWORDS.contains(&upper.as_str()) {
                push_styled(&mut out, KEYWORD_STYLE, &upper);
                if upper == "DEF" {
                    after_def = true;
                } else if upper == "CALL" || (after_def && upper == "SUB") {
                    expect_sub_name = true;
                    after_def = false;
                } else if after_def {
                    after_def = false;
                }
            } else if is_non_reserved_known_word(&upper) || upper.starts_with("FN") {
                push_styled(&mut out, OTHER_STYLE, &upper);
                after_def = false;
            } else {
                push_styled(&mut out, VARIABLE_STYLE, &word);
                after_def = false;
            }
            continue;
        }
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
            || is_signed_number_start(&chars, i)
            || (ch == '&' && i + 1 < chars.len() && matches!(chars[i + 1], 'h' | 'H' | 'x' | 'X'))
        {
            let start = i;
            if ch == '&' {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let token: String = chars[start..i].iter().collect();
                if token
                    .get(0..2)
                    .is_some_and(|p| p.eq_ignore_ascii_case("&h"))
                {
                    out.push_str(HEADER_STYLE);
                    out.push_str("&H");
                    out.push_str(HEX_STYLE);
                    out.push_str(&token[2..].to_ascii_uppercase());
                    out.push_str(RESET);
                } else {
                    out.push_str(HEADER_STYLE);
                    out.push_str("&X");
                    out.push_str(BIN_STYLE);
                    out.push_str(&token[2..]);
                    out.push_str(RESET);
                }
            } else {
                i += 1;
                if matches!(ch, '+' | '-') {
                    i += 1;
                }
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                    i += 1;
                    if i < chars.len() && matches!(chars[i], '+' | '-') {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                push_styled(
                    &mut out,
                    NUMBER_STYLE,
                    &chars[start..i].iter().collect::<String>(),
                );
            }
            continue;
        }
        let other = ch.to_string();
        push_styled(&mut out, OTHER_STYLE, &other);
        i += 1;
    }
    out
}

fn add_bas_extension_to_leading_file_command(code: &str) -> String {
    let trimmed_start = code.trim_start();
    let leading_ws = code.len() - trimmed_start.len();
    let commands = ["CHAIN MERGE", "CHAIN", "MERGE", "LOAD", "SAVE", "RUN"];
    for command in commands {
        let Some(rest) = trimmed_start.strip_prefix(command) else {
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('"') {
            continue;
        }
        let Some(end) = rest[1..].find('"') else {
            continue;
        };
        let path = &rest[1..end + 1];
        if Path::new(path).extension().is_some() {
            return code.to_string();
        }
        let mut out = String::new();
        out.push_str(&code[..leading_ws]);
        out.push_str(command);
        out.push_str(" \"");
        out.push_str(path);
        out.push_str(".bas\"");
        out.push_str(&rest[end + 2..]);
        return out;
    }
    code.to_string()
}

fn split_line_number(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim_start();
    let skipped = text.len() - trimmed.len();
    let digits = trimmed
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()?;
    if digits == 0 {
        return None;
    }
    let mut end = skipped + digits;
    let ws_start = end;
    while end < text.len() {
        let Some(ch) = text[end..].chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        end += ch.len_utf8();
    }
    if end == ws_start {
        None
    } else {
        Some((&text[..end], &text[end..]))
    }
}

fn split_single_quote_comment(text: &str) -> (&str, Option<(usize, &str)>) {
    let mut in_string = false;
    for (idx, ch) in text.char_indices() {
        if ch == '"' {
            in_string = !in_string;
        } else if ch == '\'' && !in_string {
            let spaces = text[..idx]
                .chars()
                .rev()
                .take_while(|ch| ch.is_whitespace())
                .count();
            return (&text[..idx - spaces], Some((spaces, &text[idx + 1..])));
        }
    }
    (text, None)
}

fn push_styled(out: &mut String, style: &str, text: &str) {
    out.push_str(style);
    out.push_str(text);
    out.push_str(RESET);
}

fn is_known_word(word: &str) -> bool {
    KEYWORDS.contains(&word) || is_non_reserved_known_word(word)
}

fn is_non_reserved_known_word(word: &str) -> bool {
    FUNCTIONS.contains(&word)
        || PRINT_FUNCTIONS.contains(&word)
        || NUMERIC_CONSTANTS.contains(&word)
        || OPERATORS.contains(&word)
}

pub fn is_known_basic_word(word: &str) -> bool {
    is_known_word(word)
}

fn apply_identifier_case_for_display(source: &str, cases: &HashMap<String, String>) -> String {
    if cases.is_empty() {
        return source.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0usize;
    let mut in_string = false;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            i += 1;
            continue;
        }
        if !in_string && ch == '\'' {
            out.extend(chars[i..].iter());
            break;
        }
        if !in_string && is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if word.eq_ignore_ascii_case("REM") && token_boundary(&chars, start, i) {
                out.push_str(&word);
                out.extend(chars[i..].iter());
                break;
            } else if let Some(display) = cases.get(&word.to_ascii_uppercase()) {
                out.push_str(display);
            } else {
                out.push_str(&word);
            }
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn token_boundary(chars: &[char], start: usize, end: usize) -> bool {
    let before = start == 0 || !is_ident_char(chars[start - 1]);
    let after = end >= chars.len() || !is_ident_char(chars[end]);
    before && after
}

fn is_signed_number_start(chars: &[char], index: usize) -> bool {
    let sign = chars[index];
    if !matches!(sign, '+' | '-') {
        return false;
    }
    if index > 0 && is_ident_char(chars[index - 1]) {
        return false;
    }
    let Some(next) = chars.get(index + 1).copied() else {
        return false;
    };
    next.is_ascii_digit()
        || (next == '.'
            && chars
                .get(index + 2)
                .is_some_and(|after_dot| after_dot.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_tracks_inserted_command_space_without_jumping_to_bas_suffix() {
        let load = "load\"a";
        assert_eq!(normalize_code(load), "LOAD \"a.bas\"");
        assert_eq!(normalized_cursor_position(load, load.chars().count()), 7);

        let save = "save\"foo";
        assert_eq!(normalize_code(save), "SAVE \"foo.bas\"");
        assert_eq!(normalized_cursor_position(save, save.chars().count()), 9);
    }

    #[test]
    fn cursor_tracks_auto_closing_quote_for_non_bas_commands() {
        let cd = "cd\"a";
        assert_eq!(normalize_code(cd), "CD \"a\"");
        assert_eq!(normalized_cursor_position(cd, cd.chars().count()), 5);
    }

    #[test]
    fn cursor_after_explicit_file_quote_tracks_completed_bas_name() {
        let load = "load\"a\"";
        assert_eq!(normalize_code(load), "LOAD \"a.bas\"");
        assert_eq!(normalized_cursor_position(load, load.chars().count()), 12);
    }

    #[test]
    fn cursor_after_trailing_colon_does_not_create_phantom_spacing() {
        let line = "10 print 1:";
        let normalized = normalize_code(line);
        assert_eq!(normalized, "10 PRINT 1:");
        assert_eq!(
            normalized_cursor_position(line, line.chars().count()),
            normalized.chars().count()
        );
    }

    #[test]
    fn ctrl_c_accepts_shifted_c_from_caps_lock() {
        assert!(is_ctrl_c_key('c', KeyModifiers::CONTROL));
        assert!(is_ctrl_c_key('C', KeyModifiers::CONTROL));
        assert!(!is_ctrl_c_key('c', KeyModifiers::NONE));
    }

    #[test]
    fn history_navigation_walks_up_down_and_restores_draft() {
        let entries = vec!["LIST".to_string(), "RUN".to_string()];
        let mut history = HistoryNavigation::default();
        let draft: Vec<char> = "LO".chars().collect();

        assert_eq!(
            history.previous(&entries, &draft).unwrap(),
            "RUN".chars().collect::<Vec<_>>()
        );
        assert_eq!(
            history.previous(&entries, &[]).unwrap(),
            "LIST".chars().collect::<Vec<_>>()
        );
        assert_eq!(
            history.next(&entries).unwrap(),
            "RUN".chars().collect::<Vec<_>>()
        );
        assert_eq!(history.next(&entries).unwrap(), draft);
    }

    #[test]
    fn raw_highlight_keeps_editor_text_unformatted() {
        let source = "10 print 1:";
        assert_eq!(syntax_highlight_raw_with_cases(source, false, None), source);
        assert_eq!(
            syntax_highlight_with_cases(source, false, None),
            "10 PRINT 1:"
        );
    }

    #[test]
    fn fullscreen_editor_splits_and_joins_lines() {
        let lines = vec!["10 PRINT 1".to_string()];
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_col = 2;

        editor.insert_newline();
        assert_eq!(
            editor.lines_as_strings(),
            vec!["10".to_string(), " PRINT 1".to_string()]
        );

        editor.backspace();
        assert_eq!(editor.lines_as_strings(), lines);
    }

    #[test]
    fn fullscreen_editor_copies_deletes_and_pastes_selection() {
        let lines = vec!["10 PRINT 1".to_string()];
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_col = 3;
        editor.select_right();
        editor.select_right();
        editor.select_right();
        editor.select_right();
        editor.select_right();

        assert!(editor.copy_selection());
        assert_eq!(editor.clipboard, "PRINT");

        editor.delete();
        assert_eq!(editor.lines_as_strings(), vec!["10  1".to_string()]);
        assert_eq!(editor.cursor_col, 3);

        assert!(editor.paste_clipboard());
        assert_eq!(editor.lines_as_strings(), lines);
    }

    #[test]
    fn fullscreen_editor_renumbers_visible_order_and_references() {
        let lines = vec![
            "100 GOTO 295".to_string(),
            "110 PRINT \"A\"".to_string(),
            "295 PRINT \"MOVED\"".to_string(),
            "296 GOSUB 330".to_string(),
            "120 END".to_string(),
            "330 RETURN".to_string(),
        ];
        let mut editor = BasicEditor::new(&lines);

        editor.renumber_visible_lines().unwrap();

        assert_eq!(
            editor.lines_as_strings(),
            vec![
                "100 GOTO 120".to_string(),
                "110 PRINT \"A\"".to_string(),
                "120 PRINT \"MOVED\"".to_string(),
                "130 GOSUB 150".to_string(),
                "140 END".to_string(),
                "150 RETURN".to_string(),
            ]
        );
        assert!(editor.dirty);

        assert!(editor.undo());
        assert_eq!(editor.lines_as_strings(), lines);
    }

    #[test]
    fn fullscreen_editor_renum_rejects_duplicate_line_numbers() {
        let lines = vec!["10 PRINT 1".to_string(), "10 PRINT 2".to_string()];
        let mut editor = BasicEditor::new(&lines);

        assert_eq!(
            editor.renumber_visible_lines().unwrap_err(),
            "duplicate line number"
        );
        assert_eq!(editor.lines_as_strings(), lines);
        assert!(!editor.dirty);
    }

    #[test]
    fn fullscreen_editor_find_selects_case_insensitive_match() {
        let lines = vec!["10 print 1".to_string(), "20 GOTO 10".to_string()];
        let mut editor = BasicEditor::new(&lines);

        assert_eq!(editor.find_next("PRINT"), Some(EditorFindResult::Found));
        assert_eq!(
            editor.selection_anchor,
            Some(EditorPosition { line: 0, col: 3 })
        );
        assert_eq!(editor.cursor_line, 0);
        assert_eq!(editor.cursor_col, 8);

        assert_eq!(editor.find_next("goto"), Some(EditorFindResult::Found));
        assert_eq!(
            editor.selection_anchor,
            Some(EditorPosition { line: 1, col: 3 })
        );
        assert_eq!(editor.cursor_line, 1);
        assert_eq!(editor.cursor_col, 7);
    }

    #[test]
    fn fullscreen_editor_find_wraps_to_top() {
        let lines = vec!["10 PRINT 1".to_string(), "20 END".to_string()];
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_line = 1;
        editor.cursor_col = 6;

        assert_eq!(editor.find_next("print"), Some(EditorFindResult::Wrapped));
        assert_eq!(
            editor.selection_anchor,
            Some(EditorPosition { line: 0, col: 3 })
        );
        assert_eq!(editor.cursor_line, 0);
        assert_eq!(editor.cursor_col, 8);
    }

    #[test]
    fn fullscreen_editor_replace_search_starts_at_document_top() {
        let lines = vec!["10 PRINT 1".to_string(), "20 PRINT 2".to_string()];
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_line = 1;
        editor.cursor_col = "20 PRINT 2".chars().count();

        assert!(editor.find_first("print"));
        assert_eq!(
            editor.selection_anchor,
            Some(EditorPosition { line: 0, col: 3 })
        );
        assert_eq!(editor.cursor_line, 0);
        assert_eq!(editor.cursor_col, 8);
    }

    #[test]
    fn fullscreen_editor_replace_selected_match_is_undoable() {
        let lines = vec!["10 PRINT 1".to_string(), "20 PRINT 2".to_string()];
        let mut editor = BasicEditor::new(&lines);

        assert_eq!(editor.find_next("print"), Some(EditorFindResult::Found));
        assert!(editor.replace_selected_match("print", "INPUT"));
        assert_eq!(
            editor.lines_as_strings(),
            vec!["10 INPUT 1".to_string(), "20 PRINT 2".to_string()]
        );
        assert!(editor.dirty);
        assert_eq!(editor.selection_anchor, None);
        assert_eq!(editor.cursor_line, 0);
        assert_eq!(editor.cursor_col, 8);

        assert!(editor.undo());
        assert_eq!(editor.lines_as_strings(), lines);
        assert_eq!(
            editor.selection_anchor,
            Some(EditorPosition { line: 0, col: 3 })
        );
        assert_eq!(editor.cursor_line, 0);
        assert_eq!(editor.cursor_col, 8);
        assert!(!editor.dirty);
    }

    #[test]
    fn fullscreen_editor_replace_all_from_selection_stops_at_document_end() {
        let lines = vec![
            "10 PRINT 1".to_string(),
            "20 PRINT 2".to_string(),
            "30 PRINT 3".to_string(),
        ];
        let mut editor = BasicEditor::new(&lines);

        assert_eq!(editor.find_next("print"), Some(EditorFindResult::Found));
        assert!(editor.find_next_without_wrap("print"));
        assert_eq!(
            editor.replace_all_from_selection_to_end("print", "INPUT"),
            2
        );

        assert_eq!(
            editor.lines_as_strings(),
            vec![
                "10 PRINT 1".to_string(),
                "20 INPUT 2".to_string(),
                "30 INPUT 3".to_string(),
            ]
        );

        assert!(editor.undo());
        assert_eq!(editor.lines_as_strings(), lines);
    }

    #[test]
    fn fullscreen_editor_undo_redo_restores_text_and_dirty_state() {
        let lines = vec!["10 PRINT 1".to_string()];
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_col = 2;

        editor.insert_text(" REM");
        assert_eq!(
            editor.lines_as_strings(),
            vec!["10 REM PRINT 1".to_string()]
        );
        assert!(editor.dirty);

        assert!(editor.undo());
        assert_eq!(editor.lines_as_strings(), lines);
        assert!(!editor.dirty);

        assert!(editor.redo());
        assert_eq!(
            editor.lines_as_strings(),
            vec!["10 REM PRINT 1".to_string()]
        );
        assert!(editor.dirty);
    }

    #[test]
    fn fullscreen_editor_selection_rendering_preserves_reset_styles() {
        let rendered = format!("a{RESET}bc");
        let selected = apply_selection_to_rendered(&rendered, true, Some((0, 3)));

        assert!(selected.starts_with(SELECTION_STYLE));
        assert!(selected.contains(&format!("{RESET}{SELECTION_STYLE}")));
        assert!(selected.ends_with(SELECTION_END_STYLE));
    }

    #[test]
    fn fullscreen_editor_scrolls_to_keep_cursor_visible() {
        let lines = (0..10).map(|i| format!("{i}")).collect::<Vec<_>>();
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_line = 7;
        editor.cursor_col = 5;

        editor.ensure_cursor_visible(4, 3);

        assert_eq!(editor.top_line, 5);
        assert_eq!(editor.left_col, 2);
    }

    #[test]
    fn fullscreen_editor_moves_to_document_edges() {
        let lines = vec![
            "10 PRINT 1".to_string(),
            "20 PRINT 22".to_string(),
            "30 PRINT 333".to_string(),
        ];
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_line = 1;
        editor.cursor_col = 4;

        editor.move_document_end();
        assert_eq!(editor.cursor_line, 2);
        assert_eq!(editor.cursor_col, "30 PRINT 333".chars().count());

        editor.move_document_start();
        assert_eq!(editor.cursor_line, 0);
        assert_eq!(editor.cursor_col, 0);
    }

    #[test]
    fn editor_status_places_position_at_right_edge() {
        let line = editor_status_line("F12 Apply", true, 12, 34, 24);
        assert_eq!(line.chars().count(), 24);
        assert!(line.ends_with("Ln 12 Col 34"));

        let narrow = editor_status_line("F12 Apply", false, 123, 456, 8);
        assert_eq!(narrow, "Ln 123 C");
    }
}
