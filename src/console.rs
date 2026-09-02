#[cfg(unix)]
use crate::keyboard::TerminalInputDecoder;
use crate::language;
use crate::lexer::split_command_ranges;
use crossterm::cursor::{Hide, MoveTo, MoveToColumn, Show};
use crossterm::event::{poll, read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};
use std::cell::Cell;
#[cfg(test)]
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
#[cfg(windows)]
use std::ffi::c_void;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const GRAY: &str = "\x1b[90m";
const TAN: &str = "\x1b[38;5;214m";
const ORCHID: &str = "\x1b[38;5;165m";
const WHEAT: &str = "\x1b[38;5;229m";
const SILVER: &str = "\x1b[38;5;248m";
const VIVID_GREEN: &str = "\x1b[38;5;34m";
const VIVID_ORANGE: &str = "\x1b[38;5;166m";
const VIVID_GOLD: &str = "\x1b[38;5;172m";
const DARK_HEADER: &str = "\x1b[38;5;238m";

const KEYWORD_STYLE: &str = "\x1b[1m\x1b[3m\x1b[97m";
const ERROR_STYLE: &str = "\x1b[3m\x1b[31m";
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

const LIGHT_KEYWORD_STYLE: &str = "\x1b[1m\x1b[3m\x1b[30m";
const LIGHT_PROMPT_STYLE: &str = VIVID_GREEN;
const LIGHT_COMMENT_STYLE: &str = VIVID_GREEN;
const LIGHT_LINE_NUMBER_STYLE: &str = VIVID_ORANGE;
const LIGHT_VARIABLE_STYLE: &str = "\x1b[1m\x1b[38;5;33m";
const LIGHT_NUMBER_STYLE: &str = VIVID_ORANGE;
const LIGHT_STRING_STYLE: &str = ORCHID;
const LIGHT_HEX_STYLE: &str = VIVID_GOLD;
const LIGHT_BIN_STYLE: &str = VIVID_GOLD;
const LIGHT_OTHER_STYLE: &str = SILVER;
const LIGHT_HEADER_STYLE: &str = DARK_HEADER;
const LIGHT_ERROR_STYLE: &str = "\x1b[3m\x1b[38;5;160m";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SyntaxTheme {
    Dark,
    Light,
}

#[derive(Clone, Copy)]
struct SyntaxPalette {
    keyword: &'static str,
    prompt: &'static str,
    comment: &'static str,
    line_number: &'static str,
    variable: &'static str,
    number: &'static str,
    string: &'static str,
    hex: &'static str,
    bin: &'static str,
    other: &'static str,
    header: &'static str,
    error: &'static str,
}

const DARK_SYNTAX_PALETTE: SyntaxPalette = SyntaxPalette {
    keyword: KEYWORD_STYLE,
    prompt: PROMPT_STYLE,
    comment: COMMENT_STYLE,
    line_number: LINE_NUMBER_STYLE,
    variable: VARIABLE_STYLE,
    number: NUMBER_STYLE,
    string: STRING_STYLE,
    hex: HEX_STYLE,
    bin: BIN_STYLE,
    other: OTHER_STYLE,
    header: HEADER_STYLE,
    error: ERROR_STYLE,
};

const LIGHT_SYNTAX_PALETTE: SyntaxPalette = SyntaxPalette {
    keyword: LIGHT_KEYWORD_STYLE,
    prompt: LIGHT_PROMPT_STYLE,
    comment: LIGHT_COMMENT_STYLE,
    line_number: LIGHT_LINE_NUMBER_STYLE,
    variable: LIGHT_VARIABLE_STYLE,
    number: LIGHT_NUMBER_STYLE,
    string: LIGHT_STRING_STYLE,
    hex: LIGHT_HEX_STYLE,
    bin: LIGHT_BIN_STYLE,
    other: LIGHT_OTHER_STYLE,
    header: LIGHT_HEADER_STYLE,
    error: LIGHT_ERROR_STYLE,
};

pub fn ansi_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    match std::env::var("AVL_BASIC_COLOR") {
        Ok(value)
            if matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on" | "always"
            ) =>
        {
            true
        }
        Ok(value)
            if matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off" | "never"
            ) =>
        {
            false
        }
        _ => io::stdout().is_terminal(),
    }
}

fn current_syntax_palette() -> SyntaxPalette {
    syntax_palette_for(current_syntax_theme())
}

fn current_syntax_theme() -> SyntaxTheme {
    syntax_theme_from_env_value(std::env::var("AVL_BASIC_THEME").ok().as_deref())
}

fn syntax_palette_for(theme: SyntaxTheme) -> SyntaxPalette {
    match theme {
        SyntaxTheme::Dark => DARK_SYNTAX_PALETTE,
        SyntaxTheme::Light => LIGHT_SYNTAX_PALETTE,
    }
}

fn syntax_theme_from_env_value(value: Option<&str>) -> SyntaxTheme {
    match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("light") => SyntaxTheme::Light,
        _ => SyntaxTheme::Dark,
    }
}

pub fn install_ctrl_c_handler() -> io::Result<()> {
    let result = CTRL_C_HANDLER.get_or_init(|| {
        ctrlc::set_handler(|| {
            INTERRUPT_REQUESTED.store(true, Ordering::Relaxed);
        })
        .map_err(|err| err.to_string())
    });
    match result {
        Ok(()) => Ok(()),
        Err(message) => Err(io::Error::new(io::ErrorKind::Other, message.clone())),
    }
}

pub fn take_interrupt_requested() -> bool {
    INTERRUPT_REQUESTED.load(Ordering::Relaxed)
        && INTERRUPT_REQUESTED.swap(false, Ordering::Relaxed)
}

pub fn interrupt_requested() -> bool {
    INTERRUPT_REQUESTED.load(Ordering::Relaxed)
}

pub fn clear_interrupt_requested() {
    INTERRUPT_REQUESTED.store(false, Ordering::Relaxed);
}

pub fn flush_pending_input() {
    clear_interrupt_requested();
    runtime_raw::clear_pending_input();
    flush_platform_input();
}

pub fn request_interrupt() {
    INTERRUPT_REQUESTED.store(true, Ordering::Relaxed);
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
        let palette = current_syntax_palette();
        format!("{}{plain}{RESET}", palette.prompt)
    } else {
        plain.to_string()
    }
}

pub fn error_text(ansi: bool, text: &str) -> String {
    if ansi {
        let palette = current_syntax_palette();
        format!("{}{text}{RESET}", palette.error)
    } else {
        text.to_string()
    }
}

pub fn trace_text(ansi: bool, line: i32) -> String {
    let text = format!("[{line}]");
    if ansi {
        let palette = current_syntax_palette();
        format!("{}{text}{RESET}", palette.line_number)
    } else {
        text
    }
}

pub fn normalize_code(code: &str) -> String {
    let contextual_immediate = contextual_immediate_command(code);
    let (main, comment) = split_single_quote_comment(code);
    let mut result = normalize_main_code(main.trim_end());
    result = add_bas_extension_to_leading_file_command(&result);
    result = format_colon_separators(&result);
    if let Some((spaces, comment)) = comment {
        let spaces = if result.trim().is_empty() {
            spaces
        } else {
            spaces.max(1)
        };
        result.push_str(&" ".repeat(spaces));
        result.push('\'');
        result.push_str(comment);
    }
    match contextual_immediate {
        Some(ContextualImmediateCommand::Help) => uppercase_leading_help(&mut result),
        None => {}
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
    highlight_normalized_code(normalize_code(line), ansi, cases, current_syntax_palette())
}

#[cfg(test)]
fn syntax_highlight_with_theme_for_test(line: &str, theme: SyntaxTheme) -> String {
    highlight_normalized_code(normalize_code(line), true, None, syntax_palette_for(theme))
}

fn syntax_highlight_editing_with_cases(
    line: &str,
    cursor: usize,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
) -> String {
    highlight_normalized_code(
        normalize_code_for_editing(line, cursor),
        ansi,
        cases,
        current_syntax_palette(),
    )
}

fn highlight_normalized_code(
    mut line: String,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    palette: SyntaxPalette,
) -> String {
    if let Some(cases) = cases {
        line = apply_identifier_case_for_display(&line, cases);
    }
    let contextual_immediate = contextual_immediate_command(&line);
    match contextual_immediate {
        Some(ContextualImmediateCommand::Help) => uppercase_leading_help(&mut line),
        None => {}
    }
    if !ansi {
        return line;
    }
    let (main, comment) = split_single_quote_comment(&line);
    let mut out = String::new();
    let mut rest = main;
    if let Some((line_no, after)) = split_line_number(rest) {
        out.push_str(palette.line_number);
        out.push_str(line_no);
        out.push_str(RESET);
        rest = after;
    }
    out.push_str(&highlight_main(rest, palette, contextual_immediate));
    if let Some((spaces, comment)) = comment {
        out.push_str(&" ".repeat(spaces));
        out.push('\'');
        out.push_str(palette.comment);
        out.push_str(comment);
        out.push_str(RESET);
    }
    out
}

fn normalize_code_for_editing(code: &str, cursor: usize) -> String {
    normalize_code_for_editing_marked(&mark_cursor(code, cursor))
        .chars()
        .filter(|ch| *ch != CURSOR_MARKER)
        .collect()
}

fn normalize_code_for_editing_marked(code: &str) -> String {
    let contextual_immediate = contextual_immediate_command_marked(code);
    let (main, comment) = split_single_quote_comment(code);
    let mut result = normalize_main_code_for_editing_marked(main.trim_end());
    result = add_bas_extension_to_leading_file_command(&result);
    result = format_colon_separators(&result);
    if let Some((spaces, comment)) = comment {
        let spaces = if result.trim().is_empty() {
            spaces
        } else {
            spaces.max(1)
        };
        result.push_str(&" ".repeat(spaces));
        result.push('\'');
        result.push_str(comment);
    }
    match contextual_immediate {
        Some(ContextualImmediateCommand::Help) => uppercase_leading_help(&mut result),
        None => {}
    }
    result
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
    let contextual_immediate = contextual_immediate_command(&line);
    let (main, comment) = split_single_quote_comment(&line);
    let mut out = String::new();
    let mut rest = main;
    let palette = current_syntax_palette();
    if let Some((line_no, after)) = split_line_number(rest) {
        out.push_str(palette.line_number);
        out.push_str(line_no);
        out.push_str(RESET);
        rest = after;
    }
    out.push_str(&highlight_main(rest, palette, contextual_immediate));
    if let Some((spaces, comment)) = comment {
        out.push_str(&" ".repeat(spaces));
        out.push('\'');
        out.push_str(palette.comment);
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
                    let result = finish_editing_buffer(&mut buffer, &mut cursor);
                    disable_raw_mode()?;
                    println!();
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
                KeyCode::Char(ch) if should_insert_key_char(ch, event.modifiers) => {
                    insert_editing_buffer_char(&mut buffer, &mut cursor, ch);
                    format_editing_separators_with_cursor(&mut buffer, &mut cursor);
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
                KeyCode::Right => {
                    if accept_editing_buffer_virtual_quote(&mut buffer, &mut cursor) {
                        format_editing_separators_with_cursor(&mut buffer, &mut cursor);
                        history.reset();
                    } else {
                        cursor = (cursor + 1).min(buffer.len());
                    }
                }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullscreenEditorSession {
    lines: Vec<String>,
    breakpoints: HashSet<i32>,
    editor_state: FullscreenEditorState,
}

impl FullscreenEditorSession {
    pub fn new(lines: Vec<String>) -> Self {
        Self::with_breakpoints(lines, HashSet::new())
    }

    pub fn with_breakpoints(lines: Vec<String>, breakpoints: HashSet<i32>) -> Self {
        let editor = BasicEditor::with_breakpoints(&lines, breakpoints);
        Self::from_editor(&editor)
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn breakpoints(&self) -> &HashSet<i32> {
        &self.breakpoints
    }

    fn from_editor(editor: &BasicEditor) -> Self {
        let lines = editor.lines_as_strings();
        let breakpoints = valid_editor_breakpoints(&lines, &editor.breakpoints);
        Self {
            lines,
            breakpoints,
            editor_state: FullscreenEditorState::from_editor(editor),
        }
    }

    fn into_editor(self) -> BasicEditor {
        let mut editor = BasicEditor::with_breakpoints(&self.lines, self.breakpoints);
        self.editor_state.restore_into(&mut editor);
        editor
    }
}

impl Default for FullscreenEditorSession {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

pub enum FullscreenEditOutcome {
    Apply(FullscreenEditorSession),
    Cancel,
}

fn valid_editor_breakpoints(lines: &[String], breakpoints: &HashSet<i32>) -> HashSet<i32> {
    let valid_lines = lines
        .iter()
        .filter_map(|line| editor_line_number(&line.chars().collect::<Vec<_>>()))
        .collect::<HashSet<_>>();
    breakpoints.intersection(&valid_lines).copied().collect()
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
    edit_fullscreen_with_idle(initial_lines, ansi, cases, &mut validate, || Ok(()))
}

pub fn edit_fullscreen_with_idle<F, I>(
    initial_lines: &[String],
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    mut validate: F,
    mut idle: I,
) -> io::Result<FullscreenEditOutcome>
where
    F: FnMut(&[String]) -> Result<(), String>,
    I: FnMut() -> io::Result<()>,
{
    edit_fullscreen_session_with_idle(
        FullscreenEditorSession::new(initial_lines.to_vec()),
        ansi,
        cases,
        &mut validate,
        &mut idle,
    )
}

pub fn edit_fullscreen_session_with_idle<F, I>(
    initial_session: FullscreenEditorSession,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    mut validate: F,
    mut idle: I,
) -> io::Result<FullscreenEditOutcome>
where
    F: FnMut(&[String]) -> Result<(), String>,
    I: FnMut() -> io::Result<()>,
{
    if !interactive_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "full-screen editor requires an interactive terminal",
        ));
    }

    let _guard = FullscreenEditorGuard::enter()?;
    let mut editor = initial_session.into_editor();
    let mut status = BasicEditor::default_help();
    render_fullscreen_editor(&mut editor, ansi, cases, &status)?;

    loop {
        if !poll(Duration::from_millis(30))? {
            idle()?;
            continue;
        }
        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Release {
                    continue;
                }
                match event.code {
                    KeyCode::F(12) => {
                        let session = editor.session();
                        match validate(session.lines()) {
                            Ok(()) => return Ok(FullscreenEditOutcome::Apply(session)),
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
                        status = run_editor_replace(&mut editor, ansi, cases, &mut idle)?;
                    }
                    KeyCode::F(7) => {
                        let initial = editor.last_find.clone();
                        let Some(query) = read_editor_prompt(
                            &mut editor,
                            ansi,
                            cases,
                            "Find: ",
                            &initial,
                            &mut idle,
                        )?
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
                    KeyCode::Char(ch) if should_insert_key_char(ch, event.modifiers) => {
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct DebugTimerInspection {
    target: i32,
    repeat: bool,
    active: bool,
    interval: Duration,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum DebugDataInspection {
    #[default]
    Empty,
    Next {
        position: usize,
        line: i32,
        line_item: usize,
        value: String,
    },
    Exhausted {
        position: usize,
        total: usize,
    },
}

impl From<&crate::debugger::DebugDataSnapshot> for DebugDataInspection {
    fn from(data: &crate::debugger::DebugDataSnapshot) -> Self {
        match data {
            crate::debugger::DebugDataSnapshot::Empty => Self::Empty,
            crate::debugger::DebugDataSnapshot::Next {
                position,
                line,
                line_item,
                value,
            } => Self::Next {
                position: *position,
                line: *line,
                line_item: *line_item,
                value: format_debug_value(value),
            },
            crate::debugger::DebugDataSnapshot::Exhausted { position, total } => Self::Exhausted {
                position: *position,
                total: *total,
            },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct DebugInspectionState {
    variables: HashMap<String, String>,
    arrays: HashMap<String, crate::debugger::DebugArraySummary>,
    err: i32,
    erl: i32,
    timers: HashMap<i32, DebugTimerInspection>,
    data: DebugDataInspection,
}

impl DebugInspectionState {
    fn from_snapshot(snapshot: &crate::debugger::DebugSnapshot) -> Self {
        Self {
            variables: snapshot
                .variables
                .iter()
                .chain(snapshot.array_elements.iter())
                .map(|variable| {
                    (
                        variable.name.to_ascii_uppercase(),
                        format_debug_value(&variable.value),
                    )
                })
                .collect(),
            arrays: snapshot
                .arrays
                .iter()
                .map(|array| (array.name.to_ascii_uppercase(), array.clone()))
                .collect(),
            err: snapshot.err,
            erl: snapshot.erl,
            timers: snapshot
                .timers
                .iter()
                .map(|timer| {
                    (
                        timer.number,
                        DebugTimerInspection {
                            target: timer.target,
                            repeat: timer.repeat,
                            active: timer.active,
                            interval: timer.interval,
                        },
                    )
                })
                .collect(),
            data: DebugDataInspection::from(&snapshot.data),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct DebugPanelChanges {
    variables: HashSet<String>,
    arrays: HashSet<String>,
    state: bool,
    timers: HashSet<i32>,
    data: bool,
}

#[derive(Debug, Default)]
pub(crate) struct DebugInspectionHistory {
    previous: Option<DebugInspectionState>,
}

impl DebugInspectionHistory {
    fn changes(&self, snapshot: &crate::debugger::DebugSnapshot) -> DebugPanelChanges {
        let Some(previous) = &self.previous else {
            return DebugPanelChanges::default();
        };
        let current = DebugInspectionState::from_snapshot(snapshot);
        DebugPanelChanges {
            variables: current
                .variables
                .iter()
                .filter(|(name, value)| previous.variables.get(*name) != Some(*value))
                .map(|(name, _)| name.clone())
                .collect(),
            arrays: current
                .arrays
                .iter()
                .filter(|(name, value)| previous.arrays.get(*name) != Some(*value))
                .map(|(name, _)| name.clone())
                .collect(),
            state: (current.err, current.erl) != (previous.err, previous.erl),
            timers: current
                .timers
                .iter()
                .filter(|(number, value)| previous.timers.get(number) != Some(*value))
                .map(|(number, _)| *number)
                .collect(),
            data: current.data != previous.data,
        }
    }

    fn remember(&mut self, snapshot: &crate::debugger::DebugSnapshot) {
        self.previous = Some(DebugInspectionState::from_snapshot(snapshot));
    }
}

pub fn debug_fullscreen(
    lines: &[String],
    breakpoints: &mut HashSet<i32>,
    snapshot: &crate::debugger::DebugSnapshot,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
) -> io::Result<crate::debugger::DebugAction> {
    debug_fullscreen_with_idle(lines, breakpoints, snapshot, ansi, cases, || Ok(()))
}

pub fn debug_fullscreen_with_idle<I>(
    lines: &[String],
    breakpoints: &mut HashSet<i32>,
    snapshot: &crate::debugger::DebugSnapshot,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    idle: I,
) -> io::Result<crate::debugger::DebugAction>
where
    I: FnMut() -> io::Result<()>,
{
    let terminal = DebugTerminalSession::new();
    debug_fullscreen_with_idle_and_changes(
        lines,
        breakpoints,
        snapshot,
        ansi,
        cases,
        &terminal,
        &DebugPanelChanges::default(),
        idle,
    )
}

pub(crate) fn debug_fullscreen_with_idle_and_history<I>(
    lines: &[String],
    breakpoints: &mut HashSet<i32>,
    snapshot: &crate::debugger::DebugSnapshot,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    terminal: &DebugTerminalSession,
    history: &mut DebugInspectionHistory,
    idle: I,
) -> io::Result<crate::debugger::DebugAction>
where
    I: FnMut() -> io::Result<()>,
{
    let changes = history.changes(snapshot);
    let result = debug_fullscreen_with_idle_and_changes(
        lines,
        breakpoints,
        snapshot,
        ansi,
        cases,
        terminal,
        &changes,
        idle,
    );
    if result.is_ok() {
        history.remember(snapshot);
    }
    result
}

fn debug_fullscreen_with_idle_and_changes<I>(
    lines: &[String],
    breakpoints: &mut HashSet<i32>,
    snapshot: &crate::debugger::DebugSnapshot,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    terminal: &DebugTerminalSession,
    changes: &DebugPanelChanges,
    mut idle: I,
) -> io::Result<crate::debugger::DebugAction>
where
    I: FnMut() -> io::Result<()>,
{
    if !interactive_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "integrated debugger requires an interactive terminal",
        ));
    }

    let _runtime_raw_suspend = suspend_runtime_raw_mode()?;
    let guard = terminal.enter_pause()?;
    let mut editor = BasicEditor::with_breakpoints(lines, breakpoints.clone());
    if let Some(line_index) = editor
        .lines
        .iter()
        .position(|line| editor_line_number(line) == Some(snapshot.location.line))
    {
        editor.cursor_line = line_index;
        let (cols, rows) = size().unwrap_or((80, 24));
        let layout = debug_layout(cols.max(1) as usize, rows.max(1) as usize);
        if let Some(statement_col) = reveal_debug_statement(
            &mut editor,
            line_index,
            snapshot.location.statement,
            snapshot.location.source_span.as_ref(),
            layout.code_cols,
            true,
        ) {
            editor.cursor_col = statement_col;
        } else {
            editor.cursor_col = 0;
        }
        editor.top_line = line_index.saturating_sub(layout.code_rows / 2);
    }

    let mut panel_scroll = 0usize;
    let mut status = debug_status(
        snapshot,
        editor.current_line_number(),
        terminal_debug_layout().has_inspector(),
    );
    render_fullscreen_debugger(
        &mut editor,
        snapshot,
        ansi,
        cases,
        &status,
        &mut panel_scroll,
        changes,
    )?;

    loop {
        if interrupt_requested() {
            clear_interrupt_requested();
            *breakpoints = editor.breakpoints;
            let action = crate::debugger::DebugAction::Abort;
            guard.finish(action)?;
            return Ok(action);
        }
        if !poll(Duration::from_millis(30))? {
            idle()?;
            continue;
        }
        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Release {
                    continue;
                }
                if let Some(action) = debug_action_for_key(event.code, event.modifiers) {
                    if is_debugger_ctrl_c(event.code, event.modifiers) {
                        clear_interrupt_requested();
                    }
                    *breakpoints = editor.breakpoints;
                    guard.finish(action)?;
                    return Ok(action);
                }
                match event.code {
                    KeyCode::F(2) => {
                        let message = match editor.toggle_breakpoint() {
                            Ok((true, line)) => format!("Breakpoint set at {line}"),
                            Ok((false, line)) => format!("Breakpoint cleared at {line}"),
                            Err(message) => format!("Breakpoint failed: {message}"),
                        };
                        status =
                            debug_status_message(snapshot, editor.current_line_number(), &message);
                        *breakpoints = editor.breakpoints.clone();
                    }
                    KeyCode::Up => {
                        editor.move_up();
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::Down => {
                        editor.move_down();
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::Left => {
                        editor.scroll_debug_left();
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::Right => {
                        let layout = terminal_debug_layout();
                        let view_line_len = debug_viewed_line_len(&editor, snapshot);
                        editor.scroll_debug_right(layout.code_cols, view_line_len);
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::Home => {
                        editor.move_document_start();
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::End => {
                        editor.move_document_end();
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::PageUp => {
                        editor.page_up();
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::PageDown => {
                        editor.page_down();
                        status = debug_status_for_terminal(snapshot, editor.current_line_number());
                    }
                    KeyCode::BackTab => {
                        let layout = terminal_debug_layout();
                        panel_scroll =
                            advance_debug_panel_scroll(snapshot, layout, panel_scroll, true);
                        status = debug_status(
                            snapshot,
                            editor.current_line_number(),
                            layout.has_inspector(),
                        );
                    }
                    KeyCode::Tab => {
                        let layout = terminal_debug_layout();
                        panel_scroll =
                            advance_debug_panel_scroll(snapshot, layout, panel_scroll, false);
                        status = debug_status(
                            snapshot,
                            editor.current_line_number(),
                            layout.has_inspector(),
                        );
                    }
                    _ => {}
                }
            }
            Event::Resize(cols, rows) => {
                let layout = debug_layout(cols.max(1) as usize, rows.max(1) as usize);
                if editor.current_line_number() == Some(snapshot.location.line) {
                    let cursor_line = editor.cursor_line;
                    reveal_debug_statement(
                        &mut editor,
                        cursor_line,
                        snapshot.location.statement,
                        snapshot.location.source_span.as_ref(),
                        layout.code_cols,
                        false,
                    );
                }
                panel_scroll = clamp_debug_panel_scroll(snapshot, layout, panel_scroll);
                status = debug_status(
                    snapshot,
                    editor.current_line_number(),
                    layout.has_inspector(),
                );
            }
            _ => {}
        }
        render_fullscreen_debugger(
            &mut editor,
            snapshot,
            ansi,
            cases,
            &status,
            &mut panel_scroll,
            changes,
        )?;
    }
}

fn debug_action_for_key(
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Option<crate::debugger::DebugAction> {
    use crate::debugger::DebugAction;
    match code {
        KeyCode::F(1) | KeyCode::F(5) => Some(DebugAction::Continue),
        KeyCode::F(6) => Some(DebugAction::StepInto),
        KeyCode::F(7) => Some(DebugAction::StepOver),
        KeyCode::F(8) => Some(DebugAction::StepOut),
        KeyCode::Char('c' | 'C') if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(DebugAction::Abort)
        }
        KeyCode::Esc => Some(DebugAction::Abort),
        _ => None,
    }
}

fn is_debugger_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Char('c' | 'C')) && modifiers.contains(KeyModifiers::CONTROL)
}

fn debug_location_status(
    snapshot: &crate::debugger::DebugSnapshot,
    viewed_line: Option<i32>,
) -> String {
    let mut location = format!(
        "Ln {} Stmt {}",
        snapshot.location.line,
        snapshot.location.statement + 1
    );
    if let Some(line) = viewed_line.filter(|line| *line != snapshot.location.line) {
        location.push_str(&format!(" View {line}"));
    }
    location
}

fn debug_status(
    snapshot: &crate::debugger::DebugSnapshot,
    viewed_line: Option<i32>,
    has_inspector: bool,
) -> String {
    let location = debug_location_status(snapshot, viewed_line);
    let reason = debug_pause_reason_label(snapshot.reason);
    let inspect = if has_inspector {
        " Tab/Shift+Tab Inspect"
    } else {
        ""
    };
    format!("{reason} | {location} | F5 Go F6 Into F7 Over F8 Out Esc Abort F2 Break{inspect}")
}

fn debug_status_for_terminal(
    snapshot: &crate::debugger::DebugSnapshot,
    viewed_line: Option<i32>,
) -> String {
    debug_status(
        snapshot,
        viewed_line,
        terminal_debug_layout().has_inspector(),
    )
}

fn debug_status_message(
    snapshot: &crate::debugger::DebugSnapshot,
    viewed_line: Option<i32>,
    message: &str,
) -> String {
    format!(
        "{} | {} | {message}",
        debug_pause_reason_label(snapshot.reason),
        debug_location_status(snapshot, viewed_line)
    )
}

fn debug_panel_page_size() -> usize {
    4
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DebugTerminalOperation {
    EnableRawMode,
    EnterAlternateScreen,
    ShowCursor,
    LeaveAlternateScreen,
    DisableRawMode,
}

#[derive(Debug)]
enum DebugTerminalBackend {
    Real,
    #[cfg(test)]
    Recording(Rc<RefCell<Vec<DebugTerminalOperation>>>),
}

impl DebugTerminalBackend {
    fn perform(&self, operation: DebugTerminalOperation) -> io::Result<()> {
        #[cfg(test)]
        if let Self::Recording(operations) = self {
            operations.borrow_mut().push(operation);
            return Ok(());
        }

        let mut stdout = io::stdout();
        match operation {
            DebugTerminalOperation::EnableRawMode => enable_raw_mode(),
            DebugTerminalOperation::EnterAlternateScreen => {
                execute!(stdout, EnterAlternateScreen)
            }
            DebugTerminalOperation::ShowCursor => execute!(stdout, Show),
            DebugTerminalOperation::LeaveAlternateScreen => {
                execute!(stdout, LeaveAlternateScreen)
            }
            DebugTerminalOperation::DisableRawMode => disable_raw_mode(),
        }
    }
}

#[derive(Debug)]
struct DebugTerminalInner {
    alternate_screen: Cell<bool>,
    debugger_raw_mode: Cell<bool>,
    cursor_needs_show: Cell<bool>,
    backend: DebugTerminalBackend,
}

impl DebugTerminalInner {
    fn perform(&self, operation: DebugTerminalOperation) -> io::Result<()> {
        self.backend.perform(operation)
    }

    fn restore(&self) -> io::Result<()> {
        let mut first_error = None;
        if self.cursor_needs_show.get() || self.alternate_screen.get() {
            let show_result = self.perform(DebugTerminalOperation::ShowCursor);
            if show_result.is_ok() {
                self.cursor_needs_show.set(false);
            }
            remember_terminal_error(&mut first_error, show_result);
        }
        if self.alternate_screen.get() {
            let leave_result = self.perform(DebugTerminalOperation::LeaveAlternateScreen);
            if leave_result.is_ok() {
                self.alternate_screen.set(false);
            }
            remember_terminal_error(&mut first_error, leave_result);
        }
        if self.debugger_raw_mode.get() {
            let disable_result = self.perform(DebugTerminalOperation::DisableRawMode);
            if disable_result.is_ok() {
                self.debugger_raw_mode.set(false);
            }
            remember_terminal_error(&mut first_error, disable_result);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for DebugTerminalInner {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Terminal state shared by the interactive debugger and the interpreter.
///
/// The alternate screen can remain visible while a silent step executes, but
/// the debugger's raw keyboard mode is always disabled before BASIC resumes.
#[derive(Clone, Debug)]
pub(crate) struct DebugTerminalSession {
    inner: Rc<DebugTerminalInner>,
}

impl DebugTerminalSession {
    pub(crate) fn new() -> Self {
        Self {
            inner: Rc::new(DebugTerminalInner {
                alternate_screen: Cell::new(false),
                debugger_raw_mode: Cell::new(false),
                cursor_needs_show: Cell::new(false),
                backend: DebugTerminalBackend::Real,
            }),
        }
    }

    #[cfg(test)]
    fn recording() -> (Self, Rc<RefCell<Vec<DebugTerminalOperation>>>) {
        let operations = Rc::new(RefCell::new(Vec::new()));
        let session = Self {
            inner: Rc::new(DebugTerminalInner {
                alternate_screen: Cell::new(false),
                debugger_raw_mode: Cell::new(false),
                cursor_needs_show: Cell::new(false),
                backend: DebugTerminalBackend::Recording(operations.clone()),
            }),
        };
        (session, operations)
    }

    #[cfg(test)]
    pub(crate) fn recording_for_test() -> Self {
        Self::recording().0
    }

    #[cfg(test)]
    pub(crate) fn resume_action_for_test(&self, action: crate::debugger::DebugAction) {
        self.enter_pause().unwrap().finish(action).unwrap();
    }

    #[cfg(test)]
    pub(crate) fn alternate_screen_active_for_test(&self) -> bool {
        self.inner.alternate_screen.get()
    }

    #[cfg(test)]
    pub(crate) fn primary_screen_restored_for_test(&self) -> bool {
        !self.inner.alternate_screen.get() && !self.inner.debugger_raw_mode.get()
    }

    fn enter_pause(&self) -> io::Result<DebugTerminalPauseGuard> {
        if self.inner.debugger_raw_mode.get() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "debugger terminal is already paused",
            ));
        }

        self.inner.perform(DebugTerminalOperation::EnableRawMode)?;
        self.inner.debugger_raw_mode.set(true);

        if !self.inner.alternate_screen.get() {
            if let Err(err) = self
                .inner
                .perform(DebugTerminalOperation::EnterAlternateScreen)
            {
                let disable_result = self.inner.perform(DebugTerminalOperation::DisableRawMode);
                if disable_result.is_ok() {
                    self.inner.debugger_raw_mode.set(false);
                }
                return Err(err);
            }
            self.inner.alternate_screen.set(true);
        }
        self.inner.cursor_needs_show.set(true);

        Ok(DebugTerminalPauseGuard {
            terminal: self.clone(),
            active: true,
        })
    }

    /// Restores the primary screen before BASIC performs observable I/O.
    /// Returns true only for the transition that actually revealed it.
    pub(crate) fn reveal_runtime_console(&self) -> io::Result<bool> {
        if !self.inner.alternate_screen.get() {
            if self.inner.cursor_needs_show.get() {
                if let Err(err) = self.inner.perform(DebugTerminalOperation::ShowCursor) {
                    let _ = self.inner.restore();
                    return Err(err);
                }
                self.inner.cursor_needs_show.set(false);
            }
            return Ok(false);
        }
        if self.inner.debugger_raw_mode.get() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "cannot reveal the runtime console while the debugger is paused",
            ));
        }

        let mut first_error = None;
        let show_result = self.inner.perform(DebugTerminalOperation::ShowCursor);
        if show_result.is_ok() {
            self.inner.cursor_needs_show.set(false);
        }
        remember_terminal_error(&mut first_error, show_result);
        let leave_result = self
            .inner
            .perform(DebugTerminalOperation::LeaveAlternateScreen);
        if leave_result.is_ok() {
            self.inner.alternate_screen.set(false);
        }
        remember_terminal_error(&mut first_error, leave_result);
        if let Some(err) = first_error {
            let _ = self.inner.restore();
            Err(err)
        } else {
            Ok(true)
        }
    }

    fn finish_pause(&self, keep_alternate_screen: bool) -> io::Result<()> {
        let mut first_error = None;
        let show_result = self.inner.perform(DebugTerminalOperation::ShowCursor);
        if show_result.is_ok() {
            self.inner.cursor_needs_show.set(false);
        }
        remember_terminal_error(&mut first_error, show_result);
        if !keep_alternate_screen && self.inner.alternate_screen.get() {
            let leave_result = self
                .inner
                .perform(DebugTerminalOperation::LeaveAlternateScreen);
            if leave_result.is_ok() {
                self.inner.alternate_screen.set(false);
            }
            remember_terminal_error(&mut first_error, leave_result);
        }
        if self.inner.debugger_raw_mode.get() {
            let disable_result = self.inner.perform(DebugTerminalOperation::DisableRawMode);
            if disable_result.is_ok() {
                self.inner.debugger_raw_mode.set(false);
            }
            remember_terminal_error(&mut first_error, disable_result);
        }
        first_error.map_or(Ok(()), Err)
    }
}

fn remember_terminal_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if first_error.is_none() {
        if let Err(err) = result {
            *first_error = Some(err);
        }
    }
}

fn debug_action_keeps_alternate_screen(action: crate::debugger::DebugAction) -> bool {
    matches!(
        action,
        crate::debugger::DebugAction::StepInto
            | crate::debugger::DebugAction::StepOver
            | crate::debugger::DebugAction::StepOut
    )
}

struct DebugTerminalPauseGuard {
    terminal: DebugTerminalSession,
    active: bool,
}

impl DebugTerminalPauseGuard {
    fn finish(mut self, action: crate::debugger::DebugAction) -> io::Result<()> {
        self.terminal
            .finish_pause(debug_action_keeps_alternate_screen(action))?;
        self.active = false;
        Ok(())
    }
}

impl Drop for DebugTerminalPauseGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = self.terminal.inner.restore();
        }
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
const STATUS_BAR_STYLE: &str = "\x1b[30m\x1b[48;5;250m";
const STATUS_KEY_STYLE: &str = "\x1b[38;5;21m";
const STATUS_KEY_END_STYLE: &str = STATUS_BAR_STYLE;
const DEBUG_BREAKPOINT_STYLE: &str = "\x1b[1m\x1b[31m";
const DEBUG_EXECUTION_DOT_DARK_STYLE: &str = "\x1b[1m\x1b[38;5;41m";
const DEBUG_EXECUTION_DOT_LIGHT_STYLE: &str = "\x1b[1m\x1b[38;5;41m";
const DEBUG_EXECUTION_ARROW_DARK_STYLE: &str = "\x1b[1m\x1b[38;5;226m";
const DEBUG_EXECUTION_ARROW_LIGHT_STYLE: &str = "\x1b[1m\x1b[38;5;202m";
const DEBUG_CURRENT_LINE_DARK_STYLE: &str = "\x1b[48;5;236m";
const DEBUG_CURRENT_LINE_LIGHT_STYLE: &str = "\x1b[48;5;254m";
const DEBUG_PANEL_HEADER_STYLE: &str = "\x1b[1m\x1b[38;5;45m";
const DEBUG_PANEL_CHANGED_DARK_STYLE: &str = "\x1b[1m\x1b[38;5;226m";
const DEBUG_PANEL_CHANGED_LIGHT_STYLE: &str = "\x1b[1m\x1b[38;5;202m";
const DEBUG_STATEMENT_PLACEHOLDER: char = '\u{E001}';
const EDITOR_GUTTER_WIDTH: usize = 3;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EditorPosition {
    line: usize,
    col: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EditorSnapshot {
    lines: Vec<Vec<char>>,
    breakpoints: HashSet<i32>,
    cursor_line: usize,
    cursor_col: usize,
    top_line: usize,
    left_col: usize,
    dirty: bool,
    selection_anchor: Option<EditorPosition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FullscreenEditorState {
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

impl FullscreenEditorState {
    fn from_editor(editor: &BasicEditor) -> Self {
        Self {
            cursor_line: editor.cursor_line,
            cursor_col: editor.cursor_col,
            top_line: editor.top_line,
            left_col: editor.left_col,
            page_rows: editor.page_rows,
            dirty: editor.dirty,
            selection_anchor: editor.selection_anchor,
            clipboard: editor.clipboard.clone(),
            last_find: editor.last_find.clone(),
            last_replace: editor.last_replace.clone(),
            undo_stack: editor.undo_stack.clone(),
            redo_stack: editor.redo_stack.clone(),
        }
    }

    fn restore_into(self, editor: &mut BasicEditor) {
        editor.cursor_line = self.cursor_line.min(editor.lines.len().saturating_sub(1));
        editor.cursor_col = self.cursor_col.min(editor.current_line_len());
        editor.top_line = self.top_line.min(editor.lines.len().saturating_sub(1));
        editor.left_col = self.left_col;
        editor.page_rows = self.page_rows.max(1);
        editor.dirty = self.dirty;
        editor.selection_anchor = self.selection_anchor.filter(|position| {
            editor
                .lines
                .get(position.line)
                .is_some_and(|line| position.col <= line.len())
        });
        editor.clipboard = self.clipboard;
        editor.last_find = self.last_find;
        editor.last_replace = self.last_replace;
        editor.undo_stack = self.undo_stack;
        editor.redo_stack = self.redo_stack;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorFindResult {
    Found,
    Wrapped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BasicEditor {
    lines: Vec<Vec<char>>,
    breakpoints: HashSet<i32>,
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
    #[cfg(test)]
    fn new(initial_lines: &[String]) -> Self {
        Self::with_breakpoints(initial_lines, HashSet::new())
    }

    fn with_breakpoints(initial_lines: &[String], breakpoints: HashSet<i32>) -> Self {
        let mut lines: Vec<Vec<char>> = initial_lines
            .iter()
            .map(|line| line.chars().collect::<Vec<_>>())
            .collect();
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        Self {
            lines,
            breakpoints,
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

    fn session(&self) -> FullscreenEditorSession {
        FullscreenEditorSession::from_editor(self)
    }

    fn current_line_number(&self) -> Option<i32> {
        self.lines
            .get(self.cursor_line)
            .and_then(|line| editor_line_number(line))
    }

    fn toggle_breakpoint(&mut self) -> Result<(bool, i32), &'static str> {
        let line = self.current_line_number().ok_or("no BASIC line")?;
        if self.breakpoints.remove(&line) {
            Ok((false, line))
        } else {
            self.breakpoints.insert(line);
            Ok((true, line))
        }
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
            breakpoints: self.breakpoints.clone(),
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
        self.breakpoints = snapshot.breakpoints;
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

    fn format_current_line_separators_without_history(&mut self) {
        if self.cursor_line >= self.lines.len() {
            return;
        }
        let mut line = self.lines[self.cursor_line].clone();
        let mut cursor = self.cursor_col.min(line.len());
        if format_editing_separators_with_cursor(&mut line, &mut cursor) {
            self.lines[self.cursor_line] = line;
            self.cursor_col = cursor.min(self.current_line_len());
            self.dirty = true;
        }
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
        self.format_current_line_separators_without_history();
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
        self.breakpoints = self
            .breakpoints
            .iter()
            .filter_map(|line| mapping.get(line).copied())
            .collect();
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

    fn ensure_debug_view_visible(&mut self, cols: usize, rows: usize, view_line_len: usize) {
        self.page_rows = rows.max(1);
        if self.cursor_line < self.top_line {
            self.top_line = self.cursor_line;
        } else if self.cursor_line >= self.top_line + rows.max(1) {
            self.top_line = self.cursor_line + 1 - rows.max(1);
        }
        self.left_col = self.left_col.min(view_line_len.saturating_sub(cols.max(1)));
    }

    fn scroll_debug_left(&mut self) {
        self.left_col = self.left_col.saturating_sub(4);
    }

    fn scroll_debug_right(&mut self, cols: usize, view_line_len: usize) {
        let max_left = view_line_len.saturating_sub(cols.max(1));
        self.left_col = self.left_col.saturating_add(4).min(max_left);
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

fn format_editing_separators_with_cursor(buffer: &mut Vec<char>, cursor: &mut usize) -> bool {
    let mut marked = String::new();
    let mut cursor_inserted = false;
    for (idx, ch) in buffer.iter().enumerate() {
        if idx == *cursor {
            marked.push(CURSOR_MARKER);
            cursor_inserted = true;
        }
        marked.push(*ch);
    }
    if !cursor_inserted {
        marked.push(CURSOR_MARKER);
    }

    let formatted = format_editing_separators(&marked);
    if formatted == marked {
        return false;
    }

    let mut new_buffer = Vec::new();
    let mut new_cursor = None;
    for ch in formatted.chars() {
        if ch == CURSOR_MARKER {
            new_cursor = Some(new_buffer.len());
        } else {
            new_buffer.push(ch);
        }
    }
    let Some(new_cursor) = new_cursor else {
        return false;
    };

    let changed = *buffer != new_buffer || *cursor != new_cursor;
    *buffer = new_buffer;
    *cursor = new_cursor.min(buffer.len());
    changed
}

fn format_editing_separators(source: &str) -> String {
    let (main, comment) = split_single_quote_comment(source);
    let mut result = format_colon_separators(main);
    if let Some((spaces, comment)) = comment {
        let spaces = if result.trim().is_empty() {
            spaces
        } else {
            spaces.max(1)
        };
        result.push_str(&" ".repeat(spaces));
        result.push('\'');
        result.push_str(comment);
    }
    result
}

fn editor_identifier_cases(
    lines: &[Vec<char>],
    fallback: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut cases = HashMap::new();
    for line in lines {
        let text: String = line.iter().collect();
        let code = split_line_number(&text).map_or(text.as_str(), |(_, after)| after);
        record_editor_identifier_cases(code, &mut cases);
    }
    if let Some(fallback) = fallback {
        for (canonical, display) in fallback {
            cases
                .entry(canonical.clone())
                .or_insert_with(|| display.clone());
        }
    }
    cases
}

fn record_editor_identifier_cases(source: &str, cases: &mut HashMap<String, String>) {
    let (main, _) = split_single_quote_comment(source);
    let chars: Vec<char> = main.chars().collect();
    let mut i = 0usize;
    let mut in_string = false;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            in_string = !in_string;
            i += 1;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        if is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            let upper = ident.to_ascii_uppercase();
            if upper == "REM" && token_boundary(&chars, start, i) {
                break;
            }
            if !upper.starts_with("FN") && !is_known_word(&upper) {
                cases.entry(upper).or_insert(ident);
            }
            continue;
        }
        i += 1;
    }
}

struct EditorNumberedLine {
    index: usize,
    old_number: i32,
    number_start: usize,
    number_end: usize,
}

fn editor_line_number(line: &[char]) -> Option<i32> {
    let mut start = 0usize;
    while start < line.len() && line[start].is_whitespace() {
        start += 1;
    }
    let mut end = start;
    while end < line.len() && line[end].is_ascii_digit() {
        end += 1;
    }
    if end == start || (end < line.len() && !line[end].is_whitespace()) {
        return None;
    }
    let raw: String = line[start..end].iter().collect();
    raw.parse::<i32>().ok().filter(|number| *number > 0)
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

fn run_editor_replace<I>(
    editor: &mut BasicEditor,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    idle: &mut I,
) -> io::Result<String>
where
    I: FnMut() -> io::Result<()>,
{
    let find_initial = editor.last_find.clone();
    let Some(query) = read_editor_prompt(editor, ansi, cases, "Find: ", &find_initial, idle)?
    else {
        return Ok(editor.default_status());
    };
    if query.is_empty() {
        return Ok(String::from("Find text empty"));
    }

    let replace_initial = editor.last_replace.clone();
    let Some(replacement) =
        read_editor_prompt(editor, ansi, cases, "Replace: ", &replace_initial, idle)?
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
        if !poll(Duration::from_millis(30))? {
            idle()?;
            continue;
        }
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

fn read_editor_prompt<I>(
    editor: &mut BasicEditor,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    prompt: &str,
    initial: &str,
    idle: &mut I,
) -> io::Result<Option<String>>
where
    I: FnMut() -> io::Result<()>,
{
    let mut input: Vec<char> = initial.chars().collect();
    let mut cursor = input.len();
    render_fullscreen_editor_prompt(editor, ansi, cases, prompt, &input, cursor)?;

    loop {
        if !poll(Duration::from_millis(30))? {
            idle()?;
            continue;
        }
        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Release {
                    continue;
                }
                match event.code {
                    KeyCode::Enter => return Ok(Some(input.iter().collect())),
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Char(ch) if should_insert_key_char(ch, event.modifiers) => {
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
    queue!(stdout, MoveTo(0, status_row))?;
    if ansi {
        write!(stdout, "{STATUS_BAR_STYLE}{status_text}{RESET}")?;
    } else {
        write!(stdout, "{status_text}")?;
    }
    queue!(stdout, MoveTo(cursor_x as u16, status_row), Show)?;
    stdout.flush()
}

fn editor_gutter_width(cols: usize) -> usize {
    if cols > EDITOR_GUTTER_WIDTH + 1 {
        EDITOR_GUTTER_WIDTH
    } else {
        0
    }
}

fn render_editor_gutter(ansi: bool, breakpoint: bool, current: bool, viewed: bool) -> String {
    render_editor_gutter_for_theme(ansi, breakpoint, current, viewed, current_syntax_theme())
}

fn debug_execution_dot_style(theme: SyntaxTheme) -> &'static str {
    match theme {
        SyntaxTheme::Dark => DEBUG_EXECUTION_DOT_DARK_STYLE,
        SyntaxTheme::Light => DEBUG_EXECUTION_DOT_LIGHT_STYLE,
    }
}

fn debug_execution_arrow_style(theme: SyntaxTheme) -> &'static str {
    match theme {
        SyntaxTheme::Dark => DEBUG_EXECUTION_ARROW_DARK_STYLE,
        SyntaxTheme::Light => DEBUG_EXECUTION_ARROW_LIGHT_STYLE,
    }
}

fn render_editor_gutter_for_theme(
    ansi: bool,
    breakpoint: bool,
    current: bool,
    viewed: bool,
    theme: SyntaxTheme,
) -> String {
    let breakpoint_marker = if breakpoint { '●' } else { ' ' };
    let line_marker = if current {
        '●'
    } else if viewed {
        '▶'
    } else {
        ' '
    };
    if !ansi {
        return format!("{breakpoint_marker} {line_marker}");
    }

    let mut rendered = String::new();
    if breakpoint {
        rendered.push_str(DEBUG_BREAKPOINT_STYLE);
        rendered.push(breakpoint_marker);
        rendered.push_str(RESET);
    } else {
        rendered.push(' ');
    }
    rendered.push(' ');
    if current {
        rendered.push_str(debug_execution_dot_style(theme));
        rendered.push(line_marker);
        rendered.push_str(RESET);
    } else if viewed {
        rendered.push_str(DEBUG_BREAKPOINT_STYLE);
        rendered.push(line_marker);
        rendered.push_str(RESET);
    } else {
        rendered.push(' ');
    }
    rendered
}

fn debug_statement_column(
    line: &[char],
    statement: usize,
    source_span: Option<&std::ops::Range<usize>>,
) -> Option<usize> {
    let source: String = line.iter().collect();
    if let Some(span) = source_span.filter(|span| {
        span.start < span.end
            && span.end <= source.len()
            && source.is_char_boundary(span.start)
            && source.is_char_boundary(span.end)
    }) {
        return Some(source[..span.start].chars().count());
    }

    let (_, code) = split_line_number(&source)?;
    let code_start = source.len().saturating_sub(code.len());
    let commands = split_command_ranges(code);
    let statement = commands.get(statement)?;
    Some(source[..code_start + statement.start].chars().count())
}

fn debug_viewed_line_len(editor: &BasicEditor, snapshot: &crate::debugger::DebugSnapshot) -> usize {
    let Some(line) = editor.lines.get(editor.cursor_line) else {
        return 0;
    };
    if editor_line_number(line) == Some(snapshot.location.line)
        && debug_statement_column(
            line,
            snapshot.location.statement,
            snapshot.location.source_span.as_ref(),
        )
        .is_some()
    {
        line.len().saturating_add(2)
    } else {
        line.len()
    }
}

fn reveal_debug_statement(
    editor: &mut BasicEditor,
    line_index: usize,
    statement: usize,
    source_span: Option<&std::ops::Range<usize>>,
    code_cols: usize,
    force: bool,
) -> Option<usize> {
    let line = editor.lines.get(line_index)?;
    let statement_col = debug_statement_column(line, statement, source_span)?;
    let display_len = line.len().saturating_add(2);
    let visible_end = editor.left_col.saturating_add(code_cols);
    if force || statement_col < editor.left_col || statement_col.saturating_add(2) > visible_end {
        let max_left = display_len.saturating_sub(code_cols);
        editor.left_col = statement_col.saturating_sub(code_cols / 3).min(max_left);
    }
    Some(statement_col)
}

fn debug_source_window(
    line: &[char],
    left_col: usize,
    width: usize,
    statement: usize,
    source_span: Option<&std::ops::Range<usize>>,
) -> (String, Option<usize>) {
    let Some(statement_col) = debug_statement_column(line, statement, source_span) else {
        return (line.iter().skip(left_col).take(width).collect(), None);
    };
    let mut augmented = line.to_vec();
    let statement_col = statement_col.min(augmented.len());
    augmented.insert(statement_col, DEBUG_STATEMENT_PLACEHOLDER);
    augmented.insert(statement_col + 1, ' ');
    let marker_col = (statement_col >= left_col && statement_col < left_col.saturating_add(width))
        .then(|| statement_col - left_col);
    let visible = augmented
        .iter()
        .skip(left_col)
        .take(width)
        .copied()
        .collect::<String>();
    (visible, marker_col)
}

fn finish_debug_statement_marker(rendered: &str, ansi: bool, marker_col: usize) -> String {
    finish_debug_statement_marker_for_theme(rendered, ansi, marker_col, current_syntax_theme())
}

fn finish_debug_statement_marker_for_theme(
    rendered: &str,
    ansi: bool,
    marker_col: usize,
    theme: SyntaxTheme,
) -> String {
    let marker = if ansi {
        format!("{}▶{RESET}", debug_execution_arrow_style(theme))
    } else {
        String::from("▶")
    };
    let mut out = String::with_capacity(rendered.len() + marker.len());
    let mut chars = rendered.chars().peekable();
    let mut plain_col = 0usize;
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            out.push(ch);
            out.push(chars.next().unwrap());
            for next in chars.by_ref() {
                out.push(next);
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        if plain_col == marker_col && ch == DEBUG_STATEMENT_PLACEHOLDER {
            out.push_str(&marker);
        } else {
            out.push(ch);
        }
        plain_col += 1;
    }
    out
}

fn apply_debug_execution_line_style(rendered: &str, ansi: bool, width: usize) -> String {
    if !ansi {
        return fit_plain_text(rendered, width);
    }

    let style = match current_syntax_theme() {
        SyntaxTheme::Dark => DEBUG_CURRENT_LINE_DARK_STYLE,
        SyntaxTheme::Light => DEBUG_CURRENT_LINE_LIGHT_STYLE,
    };
    let mut out = String::from(style);
    let mut rest = rendered;
    while let Some(index) = rest.find(RESET) {
        let end = index + RESET.len();
        out.push_str(&rest[..end]);
        out.push_str(style);
        rest = &rest[end..];
    }
    out.push_str(rest);
    let rendered_width = visible_width(rendered);
    if rendered_width < width {
        out.push_str(&" ".repeat(width - rendered_width));
    }
    out.push_str(RESET);
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DebugPanelPlacement {
    Hidden,
    Below,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DebugLayout {
    code_rows: usize,
    panel_rows: usize,
    status_row: usize,
    gutter_width: usize,
    code_cols: usize,
    code_region_cols: usize,
    panel_cols: usize,
    panel_x: usize,
    panel_y: usize,
    panel_placement: DebugPanelPlacement,
}

impl DebugLayout {
    fn has_inspector(self) -> bool {
        self.panel_rows > 0 && self.panel_cols > 0
    }
}

fn debug_layout(cols: usize, rows: usize) -> DebugLayout {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let usable_rows = rows.saturating_sub(1);
    let panel_placement = if cols >= 110 && usable_rows >= 4 {
        DebugPanelPlacement::Right
    } else if usable_rows >= 8 {
        DebugPanelPlacement::Below
    } else {
        DebugPanelPlacement::Hidden
    };
    let (code_rows, panel_rows, code_region_cols, panel_cols, panel_x, panel_y) =
        match panel_placement {
            DebugPanelPlacement::Right => {
                let panel_cols = (cols / 3).clamp(36, 48);
                let code_region_cols = cols.saturating_sub(panel_cols + 1).max(1);
                (
                    usable_rows.max(1),
                    usable_rows,
                    code_region_cols,
                    panel_cols,
                    code_region_cols + 1,
                    0,
                )
            }
            DebugPanelPlacement::Below => {
                let panel_rows = (usable_rows / 3).clamp(4, 10);
                (
                    usable_rows.saturating_sub(panel_rows).max(1),
                    panel_rows,
                    cols,
                    cols,
                    0,
                    usable_rows.saturating_sub(panel_rows),
                )
            }
            DebugPanelPlacement::Hidden => (usable_rows.max(1), 0, cols, 0, 0, 0),
        };
    let gutter_width = editor_gutter_width(code_region_cols);
    DebugLayout {
        code_rows,
        panel_rows,
        status_row: rows - 1,
        gutter_width,
        code_cols: code_region_cols.saturating_sub(gutter_width).max(1),
        code_region_cols,
        panel_cols,
        panel_x,
        panel_y,
        panel_placement,
    }
}

fn terminal_debug_layout() -> DebugLayout {
    let (cols, rows) = size().unwrap_or((80, 24));
    debug_layout(cols.max(1) as usize, rows.max(1) as usize)
}

fn clamp_debug_panel_scroll(
    snapshot: &crate::debugger::DebugSnapshot,
    layout: DebugLayout,
    panel_scroll: usize,
) -> usize {
    if !layout.has_inspector() {
        return 0;
    }
    let panel_lines = debug_panel_lines(snapshot, layout.panel_cols);
    panel_scroll.min(panel_lines.len().saturating_sub(layout.panel_rows))
}

fn advance_debug_panel_scroll(
    snapshot: &crate::debugger::DebugSnapshot,
    layout: DebugLayout,
    panel_scroll: usize,
    backwards: bool,
) -> usize {
    if !layout.has_inspector() {
        return 0;
    }
    let page = debug_panel_page_size();
    if backwards {
        panel_scroll.saturating_sub(page)
    } else {
        let lines = debug_panel_lines(snapshot, layout.panel_cols);
        let max_scroll = lines.len().saturating_sub(layout.panel_rows);
        if max_scroll == 0 || panel_scroll >= max_scroll {
            0
        } else {
            panel_scroll.saturating_add(page).min(max_scroll)
        }
    }
}

fn render_fullscreen_debugger(
    editor: &mut BasicEditor,
    snapshot: &crate::debugger::DebugSnapshot,
    ansi: bool,
    cases: Option<&HashMap<String, String>>,
    status: &str,
    panel_scroll: &mut usize,
    changes: &DebugPanelChanges,
) -> io::Result<()> {
    let (cols, rows) = size().unwrap_or((80, 24));
    let cols = cols.max(1) as usize;
    let rows = rows.max(1) as usize;
    let layout = debug_layout(cols, rows);
    *panel_scroll = clamp_debug_panel_scroll(snapshot, layout, *panel_scroll);
    let view_line_len = debug_viewed_line_len(editor, snapshot);
    editor.ensure_debug_view_visible(layout.code_cols, layout.code_rows, view_line_len);
    let render_cases = editor_identifier_cases(&editor.lines, cases);

    let mut stdout = io::stdout();
    queue!(stdout, Hide)?;
    for screen_row in 0..layout.code_rows {
        queue!(stdout, MoveTo(0, screen_row.min(u16::MAX as usize) as u16))?;
        let mut rendered_width = 0usize;
        if let Some(line) = editor.lines.get(editor.top_line + screen_row) {
            let line_number = editor_line_number(line);
            let current = line_number == Some(snapshot.location.line);
            if layout.gutter_width > 0 {
                let gutter = render_editor_gutter(
                    ansi,
                    line_number.is_some_and(|line| editor.breakpoints.contains(&line)),
                    current,
                    editor.top_line + screen_row == editor.cursor_line,
                );
                write!(stdout, "{gutter}")?;
                rendered_width += layout.gutter_width;
            }
            let (visible, statement_marker_col) = if current {
                debug_source_window(
                    line,
                    editor.left_col,
                    layout.code_cols,
                    snapshot.location.statement,
                    snapshot.location.source_span.as_ref(),
                )
            } else {
                (
                    line.iter()
                        .skip(editor.left_col)
                        .take(layout.code_cols)
                        .collect(),
                    None,
                )
            };
            let rendered = syntax_highlight_raw_with_cases(&visible, ansi, Some(&render_cases));
            let rendered = if let Some(marker_col) = statement_marker_col {
                finish_debug_statement_marker(&rendered, ansi, marker_col)
            } else {
                rendered
            };
            let rendered = if current {
                apply_debug_execution_line_style(&rendered, ansi, layout.code_cols)
            } else {
                rendered
            };
            rendered_width += visible_width(&rendered);
            write!(stdout, "{rendered}")?;
        } else if layout.gutter_width > 0 {
            write!(stdout, "{}", " ".repeat(layout.gutter_width))?;
            rendered_width = layout.gutter_width;
        }
        if rendered_width < layout.code_region_cols {
            write!(
                stdout,
                "{}",
                " ".repeat(layout.code_region_cols - rendered_width)
            )?;
        }
    }

    if layout.has_inspector() {
        let panel_lines = debug_panel_lines_with_changes(snapshot, changes, layout.panel_cols);
        for panel_row in 0..layout.panel_rows {
            let screen_row = layout.panel_y + panel_row;
            if screen_row >= layout.status_row {
                break;
            }
            if layout.panel_placement == DebugPanelPlacement::Right {
                queue!(
                    stdout,
                    MoveTo(
                        layout.code_region_cols.min(u16::MAX as usize) as u16,
                        screen_row.min(u16::MAX as usize) as u16
                    )
                )?;
                write!(stdout, "│")?;
            }
            queue!(
                stdout,
                MoveTo(
                    layout.panel_x.min(u16::MAX as usize) as u16,
                    screen_row.min(u16::MAX as usize) as u16
                )
            )?;
            let line = panel_lines
                .get(*panel_scroll + panel_row)
                .map_or("", String::as_str);
            let line = style_debug_panel_line_for_theme(
                line,
                layout.panel_cols,
                ansi,
                current_syntax_theme(),
            );
            write!(stdout, "{line}")?;
        }
    }

    queue!(
        stdout,
        MoveTo(0, layout.status_row.min(u16::MAX as usize) as u16)
    )?;
    let status_text = fit_plain_text(status, cols);
    if ansi {
        let status_text = style_editor_status_keys(&status_text);
        write!(stdout, "{STATUS_BAR_STYLE}{status_text}{RESET}")?;
    } else {
        write!(stdout, "{status_text}")?;
    }
    stdout.flush()
}

fn debug_panel_cell_is_heading(cell: &[char]) -> bool {
    let headings = ["VARIABLES", "ARRAYS", "STACK", "STATE", "TIMERS", "DATA"];
    let text = cell.iter().collect::<String>();
    headings.contains(&text.trim())
}

fn debug_panel_column_count(width: usize) -> usize {
    if width >= 96 {
        3
    } else if width >= 60 {
        2
    } else {
        1
    }
}

fn debug_panel_column_width(width: usize, count: usize) -> usize {
    let count = count.max(1);
    let gap_width = " | ".chars().count();
    (width.saturating_sub(gap_width.saturating_mul(count.saturating_sub(1))) / count).max(1)
}

fn debug_panel_changed_style(theme: SyntaxTheme) -> &'static str {
    match theme {
        SyntaxTheme::Dark => DEBUG_PANEL_CHANGED_DARK_STYLE,
        SyntaxTheme::Light => DEBUG_PANEL_CHANGED_LIGHT_STYLE,
    }
}

fn style_debug_panel_line_for_theme(
    line: &str,
    width: usize,
    ansi: bool,
    theme: SyntaxTheme,
) -> String {
    let line = fit_plain_text(line, width);
    if !ansi || width == 0 {
        return line;
    }

    let chars = line.chars().collect::<Vec<_>>();
    let count = debug_panel_column_count(width);
    let column_width = debug_panel_column_width(width, count);
    let gap_width = " | ".chars().count();
    let mut rendered = String::with_capacity(line.len());
    let mut copied = 0usize;

    for column in 0..count {
        let start = column.saturating_mul(column_width + gap_width);
        if start >= chars.len() {
            break;
        }
        rendered.extend(chars[copied..start].iter());
        let end = (start + column_width).min(chars.len());
        let cell = &chars[start..end];
        let style = if cell.first() == Some(&'*') {
            Some(debug_panel_changed_style(theme))
        } else if debug_panel_cell_is_heading(cell) {
            Some(DEBUG_PANEL_HEADER_STYLE)
        } else {
            None
        };
        if let Some(style) = style {
            rendered.push_str(style);
            rendered.extend(cell.iter());
            rendered.push_str(RESET);
        } else {
            rendered.extend(cell.iter());
        }
        copied = end;
    }
    rendered.extend(chars[copied..].iter());
    rendered
}

fn debug_panel_lines(snapshot: &crate::debugger::DebugSnapshot, width: usize) -> Vec<String> {
    debug_panel_lines_with_changes(snapshot, &DebugPanelChanges::default(), width)
}

fn debug_panel_change_prefix(changed: bool) -> &'static str {
    if changed {
        "* "
    } else {
        "  "
    }
}

fn debug_panel_lines_with_changes(
    snapshot: &crate::debugger::DebugSnapshot,
    changes: &DebugPanelChanges,
    width: usize,
) -> Vec<String> {
    let mut values = vec![String::from("VARIABLES")];
    if snapshot.variables.is_empty() && snapshot.array_elements.is_empty() {
        values.push(String::from("  (none)"));
    } else {
        values.extend(
            snapshot
                .variables
                .iter()
                .chain(snapshot.array_elements.iter())
                .map(|variable| {
                    format!(
                        "{}{} = {}",
                        debug_panel_change_prefix(
                            changes
                                .variables
                                .contains(&variable.name.to_ascii_uppercase())
                        ),
                        variable.name,
                        format_debug_value(&variable.value)
                    )
                }),
        );
    }
    values.push(String::from("ARRAYS"));
    if snapshot.arrays.is_empty() {
        values.push(String::from("  (none)"));
    } else {
        values.extend(snapshot.arrays.iter().map(|array| {
            let dimensions = array
                .dimensions
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let alias = array
                .alias_of
                .as_deref()
                .map_or_else(String::new, |target| format!(" -> {target}"));
            format!(
                "{}{}{}({}) {} [{}]",
                debug_panel_change_prefix(
                    changes.arrays.contains(&array.name.to_ascii_uppercase())
                ),
                array.name,
                alias,
                dimensions,
                debug_array_kind_label(array.kind),
                array.elements
            )
        }));
    }

    let mut stack = vec![String::from("STACK")];
    if snapshot.stack.is_empty() {
        stack.push(String::from("  (none)"));
    } else {
        stack.extend(snapshot.stack.iter().enumerate().map(|(index, frame)| {
            let name = frame.name.as_deref().unwrap_or("");
            let line = frame
                .line
                .map_or_else(String::new, |line| format!(" @{line}"));
            let separator = if name.is_empty() { "" } else { " " };
            format!(
                "  {index}: {}{separator}{name}{line}",
                debug_frame_kind_label(frame.kind),
            )
        }));
    }
    stack.push(String::from("STATE"));
    stack.push(format!(
        "{}ERR={} ERL={}",
        debug_panel_change_prefix(changes.state),
        snapshot.err,
        snapshot.erl
    ));

    let mut timers = vec![String::from("TIMERS")];
    if snapshot.timers.is_empty() {
        timers.push(String::from("  (none)"));
    } else {
        timers.extend(snapshot.timers.iter().map(|timer| {
            format!(
                "{}#{} {} -> {} {} {}/{}ms",
                debug_panel_change_prefix(changes.timers.contains(&timer.number)),
                timer.number,
                if timer.repeat { "EVERY" } else { "AFTER" },
                timer.target,
                if timer.active { "active" } else { "stopped" },
                timer.remaining.as_millis(),
                timer.interval.as_millis()
            )
        }));
    }
    timers.push(String::from("DATA"));
    timers.push(match &snapshot.data {
        crate::debugger::DebugDataSnapshot::Empty => String::from("  (none)"),
        crate::debugger::DebugDataSnapshot::Next {
            line,
            line_item,
            value,
            ..
        } => format!(
            "{}Ln {line} Item {line_item}: {}",
            debug_panel_change_prefix(changes.data),
            format_debug_value(value)
        ),
        crate::debugger::DebugDataSnapshot::Exhausted { .. } => {
            format!("{}(end)", debug_panel_change_prefix(changes.data))
        }
    });

    let columns = match debug_panel_column_count(width) {
        3 => vec![values, stack, timers],
        2 => {
            let mut right = stack;
            right.push(String::new());
            right.extend(timers);
            vec![values, right]
        }
        _ => {
            let mut all = values;
            all.push(String::new());
            all.extend(stack);
            all.push(String::new());
            all.extend(timers);
            vec![all]
        }
    };
    join_debug_panel_columns(&columns, width.max(1))
}

fn join_debug_panel_columns(columns: &[Vec<String>], width: usize) -> Vec<String> {
    let gap = " | ";
    let count = columns.len().max(1);
    let column_width = debug_panel_column_width(width, count);
    let rows = columns.iter().map(Vec::len).max().unwrap_or(0);
    (0..rows)
        .map(|row| {
            let joined = columns
                .iter()
                .map(|column| {
                    fit_plain_text(column.get(row).map_or("", String::as_str), column_width)
                })
                .collect::<Vec<_>>()
                .join(gap);
            fit_plain_text(&joined, width)
        })
        .collect()
}

fn format_debug_value(value: &crate::debugger::DebugValue) -> String {
    match value {
        crate::debugger::DebugValue::Number(value) => value.to_string(),
        crate::debugger::DebugValue::String(value) => format!("\"{}\"", escape_debug_string(value)),
    }
}

fn escape_debug_string(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\"\""),
            '\\' => escaped.push_str("\\\\"),
            '\0' => escaped.push_str("\\0"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            ch if ch.is_control() => {
                let code = ch as u32;
                if code <= 0xff {
                    escaped.push_str(&format!("\\x{code:02X}"));
                } else {
                    escaped.push_str(&format!("\\u{{{code:04X}}}"));
                }
            }
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn debug_array_kind_label(kind: crate::debugger::DebugArrayKind) -> &'static str {
    match kind {
        crate::debugger::DebugArrayKind::Number => "NUM",
        crate::debugger::DebugArrayKind::String => "STR",
    }
}

fn debug_frame_kind_label(kind: crate::debugger::DebugFrameKind) -> &'static str {
    match kind {
        crate::debugger::DebugFrameKind::Program => "MAIN",
        crate::debugger::DebugFrameKind::Gosub => "GOSUB",
        crate::debugger::DebugFrameKind::Sub => "SUB",
        crate::debugger::DebugFrameKind::Function => "FUNCTION",
        crate::debugger::DebugFrameKind::Timer => "TIMER",
        crate::debugger::DebugFrameKind::Mouse => "MOUSE",
    }
}

fn debug_pause_reason_label(reason: crate::debugger::DebugPauseReason) -> &'static str {
    match reason {
        crate::debugger::DebugPauseReason::Breakpoint => "BREAKPOINT",
        crate::debugger::DebugPauseReason::Step => "STEP",
        crate::debugger::DebugPauseReason::PauseRequested => "PAUSE",
    }
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
    let code_cols = cols;
    editor.ensure_cursor_visible(code_cols, edit_rows);
    let render_cases = editor_identifier_cases(&editor.lines, cases);

    let mut stdout = io::stdout();
    queue!(stdout, Hide)?;
    for screen_row in 0..edit_rows {
        queue!(stdout, MoveTo(0, screen_row.min(u16::MAX as usize) as u16))?;
        let mut rendered_width = 0usize;
        if let Some(line) = editor.lines.get(editor.top_line + screen_row) {
            let line_index = editor.top_line + screen_row;
            let visible: String = line.iter().skip(editor.left_col).take(code_cols).collect();
            let visible_len = visible.chars().count();
            let selection =
                editor
                    .selection_columns_for_line(line_index)
                    .and_then(|(start, end)| {
                        let visible_start = editor.left_col;
                        let visible_end = editor.left_col.saturating_add(code_cols);
                        if end <= visible_start || start >= visible_end {
                            return None;
                        }
                        let start = start.saturating_sub(visible_start).min(visible_len);
                        let end = end.saturating_sub(visible_start).min(visible_len);
                        (start < end).then_some((start, end))
                    });
            let rendered = syntax_highlight_raw_with_cases(&visible, ansi, Some(&render_cases));
            let rendered = apply_selection_to_rendered(&rendered, ansi, selection);
            rendered_width += visible_width(&rendered);
            write!(stdout, "{rendered}")?;
        }
        // Avoid EL after a full-width write: terminals with pending autowrap
        // may clear the last cell instead of just the unused tail.
        if rendered_width < cols {
            queue!(stdout, Clear(ClearType::UntilNewLine))?;
        }
    }

    let status_row = rows.saturating_sub(1).min(u16::MAX as usize) as u16;
    queue!(stdout, MoveTo(0, status_row))?;
    let status_text = editor_status_line(
        status,
        editor.dirty,
        editor.cursor_line + 1,
        editor.cursor_col + 1,
        cols,
    );
    // The status line is already padded to the terminal width. Clearing after
    // it can erase the final cell on terminals that keep autowrap pending.
    if ansi {
        let status_text = style_editor_status_keys(&status_text);
        write!(stdout, "{STATUS_BAR_STYLE}{status_text}{RESET}")?;
    } else {
        write!(stdout, "{status_text}")?;
    }

    let cursor_x = editor
        .cursor_col
        .saturating_sub(editor.left_col)
        .min(code_cols - 1)
        .min(cols - 1) as u16;
    let cursor_y = editor
        .cursor_line
        .saturating_sub(editor.top_line)
        .min(edit_rows - 1) as u16;
    queue!(stdout, MoveTo(cursor_x, cursor_y), Show)?;
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

fn style_editor_status_keys(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0usize;
    while index < chars.len() {
        if let Some(token_len) = status_key_token_len(&chars, index) {
            out.push_str(STATUS_KEY_STYLE);
            for ch in &chars[index..index + token_len] {
                out.push(*ch);
            }
            out.push_str(STATUS_KEY_END_STYLE);
            index += token_len;
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    out
}

fn status_key_token_len(chars: &[char], start: usize) -> Option<usize> {
    const TAB_AND_BACKTAB: [char; 13] = [
        'T', 'a', 'b', '/', 'S', 'h', 'i', 'f', 't', '+', 'T', 'a', 'b',
    ];
    if chars.get(start..start + TAB_AND_BACKTAB.len()) == Some(&TAB_AND_BACKTAB) {
        return Some(TAB_AND_BACKTAB.len());
    }
    if chars.get(start..start + 3) == Some(&['T', 'a', 'b']) {
        return Some(3);
    }
    if chars.get(start..start + 3) == Some(&['E', 's', 'c']) {
        return Some(3);
    }
    status_function_key_token_len(chars, start)
}

fn status_function_key_token_len(chars: &[char], start: usize) -> Option<usize> {
    let mut index = start;
    loop {
        if chars.get(index) != Some(&'F') {
            return None;
        }
        index += 1;

        let digit_start = index;
        while chars.get(index).is_some_and(|ch| ch.is_ascii_digit()) {
            index += 1;
        }
        if index == digit_start {
            return None;
        }

        if chars.get(index) == Some(&'/')
            && chars.get(index + 1) == Some(&'F')
            && chars.get(index + 2).is_some_and(|ch| ch.is_ascii_digit())
        {
            index += 1;
            continue;
        }

        break;
    }

    Some(index - start)
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
    let rendered = syntax_highlight_editing_with_cases(&text, cursor, ansi, cases);
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

    let normalized = normalize_code_for_editing_marked(&mark_cursor(text, cursor));
    normalized
        .chars()
        .position(|ch| ch == CURSOR_MARKER)
        .unwrap_or(cursor)
}

fn mark_cursor(text: &str, cursor: usize) -> String {
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
    marked
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

fn should_insert_key_char(ch: char, modifiers: KeyModifiers) -> bool {
    if ch.is_control() {
        return false;
    }
    if !modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }
    modifiers.contains(KeyModifiers::ALT) && !ch.is_ascii_alphanumeric()
}

fn insert_editing_buffer_char(buffer: &mut Vec<char>, cursor: &mut usize, ch: char) {
    if ch == '"' && quote_at_cursor_is_closing(buffer, *cursor) {
        *cursor += 1;
        return;
    }
    buffer.insert(*cursor, ch);
    *cursor += 1;
}

fn accept_editing_buffer_virtual_quote(buffer: &mut Vec<char>, cursor: &mut usize) -> bool {
    if !cursor_on_virtual_closing_quote(buffer, *cursor) {
        return false;
    }
    buffer.insert(*cursor, '"');
    *cursor += 1;
    true
}

fn finish_editing_buffer(buffer: &mut Vec<char>, cursor: &mut usize) -> String {
    accept_editing_buffer_virtual_quote(buffer, cursor);
    buffer.iter().collect()
}

fn cursor_on_virtual_closing_quote(line: &[char], cursor: usize) -> bool {
    cursor == line.len() && line.iter().filter(|ch| **ch == '"').count() % 2 == 1
}

fn quote_at_cursor_is_closing(line: &[char], cursor: usize) -> bool {
    if cursor >= line.len() || line[cursor] != '"' {
        return false;
    }
    line[..cursor].iter().filter(|ch| **ch == '"').count() % 2 == 1
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
    use super::{RuntimeRawModeGuard, RuntimeRawModeSuspendGuard, TerminalInputDecoder};
    use std::io::{self, IsTerminal};
    use std::mem;
    use std::ptr;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    const STDIN_FD: libc::c_int = 0;

    #[derive(Default)]
    struct RawState {
        depth: usize,
        suspend_depth: usize,
        original: Option<libc::termios>,
        input_decoder: TerminalInputDecoder,
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
            state.input_decoder.clear();
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

        let mut state = state().lock().ok()?;
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
                state.input_decoder.push(byte);
                if let Some(code) = state.input_decoder.next_code(Instant::now()) {
                    return Some(code);
                }
            }
        }

        if let Some(code) = state.input_decoder.next_code(Instant::now()) {
            return Some(code);
        }

        drop(state);
        thread::sleep(Duration::from_micros(500));
        None
    }

    pub(super) fn clear_pending_input() {
        if let Ok(mut state) = state().lock() {
            state.input_decoder.clear();
        }
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

    pub(super) fn clear_pending_input() {}
}

fn normalize_main_code(code: &str) -> String {
    normalize_main_code_inner(code, false)
}

fn normalize_main_code_for_editing_marked(code: &str) -> String {
    normalize_main_code_inner(code, true)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextualImmediateCommand {
    Help,
}

fn contextual_immediate_command(line: &str) -> Option<ContextualImmediateCommand> {
    if is_help_immediate_line(line) {
        Some(ContextualImmediateCommand::Help)
    } else {
        None
    }
}

fn contextual_immediate_command_marked(line: &str) -> Option<ContextualImmediateCommand> {
    let unmarked: String = line.chars().filter(|ch| *ch != CURSOR_MARKER).collect();
    contextual_immediate_command(&unmarked)
}

pub(crate) fn is_help_immediate_line(line: &str) -> bool {
    let command = line.trim();
    let Some(prefix) = command.get(..4) else {
        return false;
    };
    if !prefix.eq_ignore_ascii_case("HELP") {
        return false;
    }
    let rest = &command[4..];
    if rest.is_empty() {
        return true;
    }
    if !rest.starts_with(char::is_whitespace) {
        return false;
    }
    let topic = rest.trim();
    if topic == "'" {
        return true;
    }
    !topic.contains(['=', ':', '\''])
}

fn uppercase_leading_help(line: &mut String) {
    let mut command_chars = 0usize;
    let mut command_started = false;
    *line = line
        .chars()
        .map(|ch| {
            if !command_started && ch.is_whitespace() {
                return ch;
            }
            command_started = true;
            if ch == CURSOR_MARKER {
                return ch;
            }
            if command_chars < 4 {
                command_chars += 1;
                ch.to_ascii_uppercase()
            } else {
                ch
            }
        })
        .collect();
}

fn normalize_main_code_inner(code: &str, preserve_marked_number: bool) -> String {
    let mut out = String::new();
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            out.push(ch);
            i += 1;
            let mut closed = false;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '"' {
                    i += 1;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed {
                out.push('"');
            }
            continue;
        }
        if is_number_start_at(&chars, i, preserve_marked_number) {
            let start = i;
            let (end, contains_marker) = scan_number_token(&chars, i, preserve_marked_number);
            i = end;
            let raw = chars[start..i].iter().collect::<String>();
            if preserve_marked_number && contains_marker {
                out.push_str(&raw);
            } else {
                out.push_str(&canonicalize_number(&raw));
            }
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
                    "LOAD"
                        | "SAVE"
                        | "RUN"
                        | "CHAIN"
                        | "MERGE"
                        | "CAT"
                        | "FILES"
                        | "CD"
                        | "PRINT"
                        | "USING"
                        | "GPRINT"
                        | "LABEL"
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

fn is_number_start_at(chars: &[char], index: usize, allow_cursor_marker: bool) -> bool {
    let mut probe = index;
    if allow_cursor_marker && chars.get(probe) == Some(&CURSOR_MARKER) {
        probe += 1;
    }
    let Some(ch) = chars.get(probe).copied() else {
        return false;
    };
    ch.is_ascii_digit()
        || (ch == '.'
            && next_non_marker(chars, probe + 1, allow_cursor_marker)
                .is_some_and(|next| next.is_ascii_digit()))
}

fn scan_number_token(chars: &[char], start: usize, allow_cursor_marker: bool) -> (usize, bool) {
    let mut index = start;
    let mut contains_marker = false;
    while index < chars.len() {
        let ch = chars[index];
        if allow_cursor_marker && ch == CURSOR_MARKER {
            contains_marker = true;
            index += 1;
        } else if ch.is_ascii_digit() || ch == '.' {
            index += 1;
        } else {
            break;
        }
    }

    if index < chars.len() && matches!(chars[index], 'e' | 'E') {
        index += 1;
        while index < chars.len() && allow_cursor_marker && chars[index] == CURSOR_MARKER {
            contains_marker = true;
            index += 1;
        }
        if index < chars.len() && matches!(chars[index], '+' | '-') {
            index += 1;
        }
        while index < chars.len() {
            let ch = chars[index];
            if allow_cursor_marker && ch == CURSOR_MARKER {
                contains_marker = true;
                index += 1;
            } else if ch.is_ascii_digit() {
                index += 1;
            } else {
                break;
            }
        }
    }

    (index, contains_marker)
}

fn next_non_marker(chars: &[char], mut index: usize, allow_cursor_marker: bool) -> Option<char> {
    while index < chars.len() {
        let ch = chars[index];
        if allow_cursor_marker && ch == CURSOR_MARKER {
            index += 1;
            continue;
        }
        return Some(ch);
    }
    None
}

fn format_colon_separators(source: &str) -> String {
    let (prefix, body) = split_line_number(source).unwrap_or(("", source));
    let statements = split_listing_statements(body);
    if !statements.changed {
        return source.to_string();
    }
    let mut formatted = format!("{prefix}{}", statements.items.join(" : "));
    if statements.trailing_separator && !statements.items.is_empty() {
        formatted.push_str(" :");
    }
    formatted
}

struct ListingStatements {
    items: Vec<String>,
    trailing_separator: bool,
    changed: bool,
}

fn split_listing_statements(code: &str) -> ListingStatements {
    let chars: Vec<char> = code.chars().collect();
    let mut statements = Vec::new();
    let mut buffer = String::new();
    let mut i = 0usize;
    let mut in_string = false;
    let mut trailing_separator = false;
    let mut changed = false;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            in_string = !in_string;
            buffer.push(ch);
            trailing_separator = false;
            i += 1;
            continue;
        }

        if !in_string && starts_with_chars(&chars, i, "REM ") {
            push_statement(&mut statements, &buffer);
            buffer.clear();
            let rem: String = chars[i..].iter().collect();
            push_statement(&mut statements, &rem);
            return ListingStatements {
                items: statements,
                trailing_separator: false,
                changed,
            };
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
                let if_block = compact_inline_colon_separators(&if_block);
                changed |= if_block != chars[i..].iter().collect::<String>();
                push_statement(&mut statements, &if_block);
                return ListingStatements {
                    items: statements,
                    trailing_separator: false,
                    changed,
                };
            }
        }

        if ch == ':' && !in_string {
            push_statement(&mut statements, &buffer);
            buffer.clear();
            trailing_separator = true;
            changed = true;
            i += 1;
            continue;
        }

        buffer.push(ch);
        if !ch.is_whitespace() {
            trailing_separator = false;
        }
        i += 1;
    }

    push_statement(&mut statements, &buffer);
    ListingStatements {
        items: statements,
        trailing_separator,
        changed,
    }
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
        .is_some_and(|slice| {
            slice
                .iter()
                .zip(needle_chars.iter())
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        })
}

fn compact_inline_colon_separators(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
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
        if ch == ':' && !in_string {
            while out.ends_with(char::is_whitespace) {
                out.pop();
            }
            out.push(':');
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            continue;
        }
        out.push(ch);
        i += 1;
    }

    out
}

fn canonicalize_number(raw: &str) -> String {
    if raw.contains('e') || raw.contains('E') {
        let Ok(value) = raw.parse::<f64>() else {
            return raw.to_string();
        };
        if !value.is_finite() {
            return raw.trim_start_matches('+').to_string();
        }
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
    canonicalize_decimal_number(raw)
}

fn canonicalize_decimal_number(raw: &str) -> String {
    let text = raw.trim_start_matches('+');
    let (negative, unsigned) = text
        .strip_prefix('-')
        .map_or((false, text), |rest| (true, rest));
    let mut parts = unsigned.split('.');
    let int_raw = parts.next().unwrap_or_default();
    let frac_raw = parts.next();
    if parts.next().is_some()
        || (!int_raw.chars().all(|ch| ch.is_ascii_digit()))
        || frac_raw.is_some_and(|frac| !frac.chars().all(|ch| ch.is_ascii_digit()))
        || (int_raw.is_empty() && frac_raw.is_none_or(str::is_empty))
    {
        return text.to_string();
    }

    let mut int_part = trim_leading_decimal_zeros(int_raw).to_string();
    let mut frac_part = frac_raw.unwrap_or_default().to_string();
    if frac_part.len() > 14 {
        let kept = frac_part[..14].to_string();
        let dropped = &frac_part[14..];
        let round_up = decimal_rounds_up_half_even(&kept, dropped);
        frac_part = kept;
        if round_up {
            (int_part, frac_part) = increment_fixed_decimal(&int_part, &frac_part, 14);
        }
    }

    while frac_part.ends_with('0') {
        frac_part.pop();
    }
    if int_part == "0" && frac_part.is_empty() {
        "0".to_string()
    } else if frac_part.is_empty() {
        format!("{}{}", if negative { "-" } else { "" }, int_part)
    } else {
        format!(
            "{}{}.{}",
            if negative { "-" } else { "" },
            int_part,
            frac_part
        )
    }
}

fn trim_leading_decimal_zeros(text: &str) -> &str {
    let trimmed = text.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

fn decimal_rounds_up_half_even(kept: &str, dropped: &str) -> bool {
    let mut chars = dropped.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first > '5' {
        return true;
    }
    if first < '5' {
        return false;
    }
    if chars.any(|ch| ch != '0') {
        return true;
    }
    kept.as_bytes()
        .last()
        .is_some_and(|digit| (digit - b'0') % 2 == 1)
}

fn increment_fixed_decimal(int_part: &str, frac_part: &str, frac_width: usize) -> (String, String) {
    let mut digits = format!("{int_part}{frac_part}").into_bytes();
    let mut index = digits.len();
    let mut carry = true;
    while carry && index > 0 {
        index -= 1;
        if digits[index] == b'9' {
            digits[index] = b'0';
        } else {
            digits[index] += 1;
            carry = false;
        }
    }
    if carry {
        digits.insert(0, b'1');
    }
    let split = digits.len().saturating_sub(frac_width);
    let int_part = String::from_utf8(digits[..split].to_vec()).unwrap_or_else(|_| "0".to_string());
    let frac_part = String::from_utf8(digits[split..].to_vec()).unwrap_or_default();
    (trim_leading_decimal_zeros(&int_part).to_string(), frac_part)
}

fn highlight_main(
    text: &str,
    palette: SyntaxPalette,
    contextual_immediate: Option<ContextualImmediateCommand>,
) -> String {
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
                palette.string,
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
            if contextual_immediate == Some(ContextualImmediateCommand::Help)
                && upper == "HELP"
                && chars[..start].iter().all(|ch| ch.is_whitespace())
            {
                push_styled(&mut out, palette.keyword, "HELP");
            } else if expect_sub_name {
                push_styled(&mut out, palette.keyword, &upper);
                expect_sub_name = false;
                after_def = false;
            } else if upper == "REM" && token_boundary(&chars, start, i) {
                push_styled(&mut out, palette.keyword, "REM");
                push_styled(
                    &mut out,
                    palette.comment,
                    &chars[i..].iter().collect::<String>(),
                );
                return out;
            } else if language::is_keyword(&upper) {
                push_styled(&mut out, palette.keyword, &upper);
                if upper == "DEF" {
                    after_def = true;
                } else if upper == "CALL" || (after_def && upper == "SUB") {
                    expect_sub_name = true;
                    after_def = false;
                } else if after_def {
                    after_def = false;
                }
            } else if is_non_reserved_known_word(&upper) || upper.starts_with("FN") {
                push_styled(&mut out, palette.other, &upper);
                after_def = false;
            } else {
                push_styled(&mut out, palette.variable, &word);
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
                    out.push_str(palette.header);
                    out.push_str("&H");
                    out.push_str(palette.hex);
                    out.push_str(&token[2..].to_ascii_uppercase());
                    out.push_str(RESET);
                } else {
                    out.push_str(palette.header);
                    out.push_str("&X");
                    out.push_str(palette.bin);
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
                    palette.number,
                    &chars[start..i].iter().collect::<String>(),
                );
            }
            continue;
        }
        let other = ch.to_string();
        push_styled(&mut out, palette.other, &other);
        i += 1;
    }
    out
}

fn add_bas_extension_to_leading_file_command(code: &str) -> String {
    let trimmed_start = code.trim_start();
    let leading_ws = code.len() - trimmed_start.len();
    let commands = ["CHAIN MERGE", "CHAIN", "MERGE", "LOAD", "SAVE", "RUN"];
    for command in commands {
        if !trimmed_start.starts_with(command) {
            continue;
        }
        let mut rest_start = leading_ws + command.len();
        let mut cursor_markers = String::new();
        while rest_start < code.len() {
            let Some(ch) = code[rest_start..].chars().next() else {
                break;
            };
            if ch == CURSOR_MARKER {
                cursor_markers.push(ch);
                rest_start += ch.len_utf8();
            } else if ch.is_whitespace() {
                rest_start += ch.len_utf8();
            } else {
                break;
            }
        }
        if !code[rest_start..].starts_with('"') {
            continue;
        }
        let path_start = rest_start + 1;
        let Some(relative_end) = code[path_start..].find('"') else {
            continue;
        };
        let path_end = path_start + relative_end;
        let path = &code[path_start..path_end];
        let path_for_extension = path.replace(CURSOR_MARKER, "");
        if Path::new(&path_for_extension).extension().is_some() {
            return code.to_string();
        }
        let mut out = String::new();
        out.push_str(&code[..leading_ws]);
        out.push_str(command);
        out.push(' ');
        out.push_str(&cursor_markers);
        out.push('"');
        out.push_str(path);
        out.push_str(".bas\"");
        out.push_str(&code[path_end + 1..]);
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
    language::is_known_word(word)
}

fn is_non_reserved_known_word(word: &str) -> bool {
    language::is_other_word(word)
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
    fn file_command_bas_completion_survives_cursor_before_quote() {
        let load = "load\"demo\"";
        let cursor = "load".chars().count();
        assert_eq!(
            normalize_code_for_editing(load, cursor),
            "LOAD \"demo.bas\""
        );
        assert_eq!(
            normalized_cursor_position(load, cursor),
            "LOAD ".chars().count()
        );

        let open_load = "load\"demo";
        let cursor = "load".chars().count();
        assert_eq!(
            normalize_code_for_editing(open_load, cursor),
            "LOAD \"demo.bas\""
        );
        assert_eq!(
            normalized_cursor_position(open_load, cursor),
            "LOAD ".chars().count()
        );
    }

    #[test]
    fn cursor_after_trailing_colon_tracks_live_separator_space() {
        let line = "10 print 1:";
        let normalized = normalize_code(line);
        assert_eq!(normalized, "10 PRINT 1 :");
        assert_eq!(
            normalized_cursor_position(line, line.chars().count()),
            normalized.chars().count()
        );
    }

    #[test]
    fn live_input_preserves_number_under_cursor_until_cursor_leaves_it() {
        let decimal = "10 PRINT 1.";
        assert_eq!(
            normalize_code_for_editing(decimal, decimal.chars().count()),
            "10 PRINT 1."
        );
        assert_eq!(normalize_code(decimal), "10 PRINT 1");
        assert_eq!(
            normalized_cursor_position(decimal, decimal.chars().count()),
            "10 PRINT 1.".chars().count()
        );

        let completed_decimal = "10 PRINT 1. + 2";
        let cursor_after_space = "10 PRINT 1. ".chars().count();
        assert_eq!(
            normalize_code_for_editing(completed_decimal, cursor_after_space),
            "10 PRINT 1 + 2"
        );

        let exponent = "10 PRINT 1E+";
        assert_eq!(
            normalize_code_for_editing(exponent, exponent.chars().count()),
            "10 PRINT 1E+"
        );

        let big = "10 PRINT 123456789012345678901234567890";
        assert_eq!(normalize_code_for_editing(big, big.chars().count()), big);

        let leading_decimal = "10 PRINT .5";
        assert_eq!(
            normalize_code_for_editing(leading_decimal, "10 PRINT .".chars().count()),
            "10 PRINT .5"
        );
        assert_eq!(
            syntax_highlight_editing_with_cases(decimal, decimal.chars().count(), false, None),
            "10 PRINT 1."
        );
    }

    #[test]
    fn ctrl_c_accepts_shifted_c_from_caps_lock() {
        assert!(is_ctrl_c_key('c', KeyModifiers::CONTROL));
        assert!(is_ctrl_c_key('C', KeyModifiers::CONTROL));
        assert!(!is_ctrl_c_key('c', KeyModifiers::NONE));
    }

    #[test]
    fn alt_gr_printable_characters_are_text_input() {
        let alt_gr = KeyModifiers::CONTROL | KeyModifiers::ALT;

        assert!(should_insert_key_char('#', alt_gr));
        assert!(should_insert_key_char('@', alt_gr));
        assert!(should_insert_key_char('|', alt_gr));
        assert!(!should_insert_key_char('c', KeyModifiers::CONTROL));
        assert!(!should_insert_key_char('c', alt_gr));
    }

    #[test]
    fn editing_buffer_skips_real_and_virtual_closing_quotes() {
        let mut closed: Vec<char> = r#"10 PRINT "A""#.chars().collect();
        let mut cursor = closed.len() - 1;
        insert_editing_buffer_char(&mut closed, &mut cursor, '"');
        assert_eq!(closed.iter().collect::<String>(), r#"10 PRINT "A""#);
        assert_eq!(cursor, closed.len());

        let mut before_opening: Vec<char> = r#"10 PRINT "A""#.chars().collect();
        let mut cursor = "10 PRINT ".chars().count();
        insert_editing_buffer_char(&mut before_opening, &mut cursor, '"');
        assert_eq!(
            before_opening.iter().collect::<String>(),
            r#"10 PRINT ""A""#
        );
        assert_eq!(cursor, "10 PRINT \"".chars().count());

        let mut open: Vec<char> = r#"10 PRINT "A"#.chars().collect();
        let mut cursor = open.len();
        assert!(accept_editing_buffer_virtual_quote(&mut open, &mut cursor));
        assert_eq!(open.iter().collect::<String>(), r#"10 PRINT "A""#);
        assert_eq!(cursor, open.len());
    }

    #[test]
    fn enter_materializes_virtual_closing_quote() {
        let mut empty: Vec<char> = "10 PRINT \"".chars().collect();
        let mut cursor = empty.len();
        assert_eq!(
            finish_editing_buffer(&mut empty, &mut cursor),
            "10 PRINT \"\""
        );
        assert_eq!(cursor, empty.len());

        let mut text: Vec<char> = "10 PRINT \"A".chars().collect();
        let mut cursor = text.len();
        assert_eq!(
            finish_editing_buffer(&mut text, &mut cursor),
            "10 PRINT \"A\""
        );
        assert_eq!(cursor, text.len());

        let mut closed: Vec<char> = "10 PRINT \"\"".chars().collect();
        let mut cursor = closed.len();
        assert_eq!(
            finish_editing_buffer(&mut closed, &mut cursor),
            "10 PRINT \"\""
        );
        assert_eq!(cursor, closed.len());
    }

    #[test]
    fn normalization_closes_empty_unfinished_string() {
        assert_eq!(normalize_code("10 print\""), "10 PRINT \"\"");
        assert_eq!(
            normalize_code_for_editing("10 print\"", "10 print\"".chars().count()),
            "10 PRINT \"\""
        );
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
            "10 PRINT 1 :"
        );
    }

    #[test]
    fn removed_showcase_commands_are_ordinary_identifiers() {
        let cases = HashMap::from([
            ("TOUR".to_string(), "tour".to_string()),
            ("SAMPLES".to_string(), "Samples".to_string()),
        ]);

        assert_eq!(
            syntax_highlight_with_cases("tour", false, Some(&cases)),
            "tour"
        );
        assert_eq!(
            syntax_highlight_with_cases("samples", false, Some(&cases)),
            "Samples"
        );
        assert_eq!(
            syntax_highlight_with_cases("PRINT tour; samples", false, Some(&cases)),
            "PRINT tour; Samples"
        );
        assert_eq!(normalize_code("tour"), "tour");
        assert_eq!(normalize_code("samples"), "samples");
        assert!(!is_known_basic_word("TOUR"));
        assert!(!is_known_basic_word("SAMPLES"));
    }

    #[test]
    fn help_is_contextual_without_becoming_a_reserved_word() {
        assert_eq!(normalize_code("help"), "HELP");
        assert_eq!(normalize_code("help right$"), "HELP RIGHT$");
        assert_eq!(normalize_code("help '"), "HELP '");
        assert_eq!(normalize_code("  help   right$  "), "  HELP   RIGHT$");
        assert_eq!(
            syntax_highlight("help right$", true),
            format!("{KEYWORD_STYLE}HELP{RESET}{OTHER_STYLE} {RESET}{OTHER_STYLE}RIGHT${RESET}")
        );
        assert_eq!(
            syntax_highlight_raw_with_cases("help right$", true, None),
            format!("{KEYWORD_STYLE}HELP{RESET}{OTHER_STYLE} {RESET}{OTHER_STYLE}RIGHT${RESET}")
        );

        for cursor in 0..="help right$".chars().count() {
            assert!(
                syntax_highlight_editing_with_cases("help right$", cursor, false, None)
                    .starts_with("HELP ")
            );
        }

        assert!(!is_known_basic_word("HELP"));
        for source in [
            "help=1",
            "help = 1",
            "10 help",
            "10 help=1",
            "PRINT help",
            "help:",
            "help ' comment",
        ] {
            let highlighted = syntax_highlight(source, true);
            assert!(
                !highlighted.contains(&format!("{KEYWORD_STYLE}HELP{RESET}")),
                "unexpected HELP highlighting for {source}: {highlighted:?}"
            );
        }
        assert_eq!(normalize_code("help=1"), "help=1");
        assert_eq!(normalize_code("help = 1"), "help = 1");
        assert_eq!(normalize_code("10 help"), "10 help");
    }

    #[test]
    fn highlighted_functions_constants_and_operators_have_help_topics() {
        for word in language::other_words() {
            assert!(
                crate::help::has_topic(word),
                "highlighted entry {word} has no HELP topic"
            );
        }
    }

    #[test]
    fn syntax_theme_is_explicit_light_or_default_dark() {
        assert_eq!(
            syntax_theme_from_env_value(Some("light")),
            SyntaxTheme::Light
        );
        assert_eq!(
            syntax_theme_from_env_value(Some(" LIGHT ")),
            SyntaxTheme::Light
        );
        assert_eq!(syntax_theme_from_env_value(Some("dark")), SyntaxTheme::Dark);
        assert_eq!(
            syntax_theme_from_env_value(Some("unknown")),
            SyntaxTheme::Dark
        );
        assert_eq!(syntax_theme_from_env_value(None), SyntaxTheme::Dark);
    }

    #[test]
    fn light_syntax_theme_uses_dark_visible_styles() {
        let highlighted =
            syntax_highlight_with_theme_for_test("10 PRINT ABS(X)+&HFF", SyntaxTheme::Light);

        assert!(highlighted.contains(&format!("{LIGHT_KEYWORD_STYLE}PRINT{RESET}")));
        assert!(highlighted.contains(&format!("{LIGHT_OTHER_STYLE}ABS{RESET}")));
        assert!(highlighted.contains(&format!("{LIGHT_VARIABLE_STYLE}X{RESET}")));
        assert!(highlighted.contains(&format!("{LIGHT_HEADER_STYLE}&H{LIGHT_HEX_STYLE}FF{RESET}")));
        assert!(!highlighted.contains(&format!("{KEYWORD_STYLE}PRINT{RESET}")));
        assert_eq!(
            syntax_palette_for(SyntaxTheme::Light).error,
            LIGHT_ERROR_STYLE
        );
        assert_eq!(syntax_palette_for(SyntaxTheme::Dark).error, ERROR_STYLE);
    }

    #[test]
    fn live_separator_formatting_updates_buffer_and_cursor() {
        let mut top_level: Vec<char> = "10 print 1:print 2".chars().collect();
        let mut cursor = top_level.len();
        assert!(format_editing_separators_with_cursor(
            &mut top_level,
            &mut cursor
        ));
        assert_eq!(top_level.iter().collect::<String>(), "10 print 1 : print 2");
        assert_eq!(cursor, top_level.len());

        let mut trailing: Vec<char> = "10 print 1:".chars().collect();
        let mut cursor = trailing.len();
        assert!(format_editing_separators_with_cursor(
            &mut trailing,
            &mut cursor
        ));
        assert_eq!(trailing.iter().collect::<String>(), "10 print 1 : ");
        assert_eq!(cursor, trailing.len());

        let mut if_body: Vec<char> = "10 if a then print 1:print 2 else print 3:print 4"
            .chars()
            .collect();
        let mut cursor = if_body.len();
        assert!(!format_editing_separators_with_cursor(
            &mut if_body,
            &mut cursor
        ));

        let mut spaced_if_body: Vec<char> = "10 if a then print 1 : print 2".chars().collect();
        let mut cursor = spaced_if_body.len();
        assert!(format_editing_separators_with_cursor(
            &mut spaced_if_body,
            &mut cursor
        ));
        assert_eq!(
            spaced_if_body.iter().collect::<String>(),
            "10 if a then print 1:print 2"
        );
        assert_eq!(cursor, spaced_if_body.len());

        let mut comment: Vec<char> = "10 print 1'comment".chars().collect();
        let mut cursor = comment.len();
        assert!(format_editing_separators_with_cursor(
            &mut comment,
            &mut cursor
        ));
        assert_eq!(comment.iter().collect::<String>(), "10 print 1 'comment");
        assert_eq!(cursor, comment.len());
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
    fn fullscreen_editor_keeps_quotes_literal_for_multiline_editing() {
        let lines = vec![r#"10 PRINT "A""#.to_string()];
        let mut editor = BasicEditor::new(&lines);
        editor.cursor_col = lines[0].chars().count() - 1;

        editor.insert_char('"');

        assert_eq!(
            editor.lines_as_strings(),
            vec![r#"10 PRINT "A"""#.to_string()]
        );
        assert_eq!(editor.cursor_col, lines[0].chars().count());
        assert!(editor.dirty);

        let open = vec![r#"10 PRINT "A"#.to_string()];
        let mut editor = BasicEditor::new(&open);
        editor.cursor_col = open[0].chars().count();

        editor.move_right();

        assert_eq!(editor.lines_as_strings(), open);
        assert_eq!(editor.cursor_col, open[0].chars().count());
        assert!(!editor.dirty);

        let mut editor = BasicEditor::new(&lines);
        editor.cursor_col = "10 PRINT ".chars().count();

        editor.insert_char('"');

        assert_eq!(
            editor.lines_as_strings(),
            vec![r#"10 PRINT ""A""#.to_string()]
        );
        assert_eq!(editor.cursor_col, "10 PRINT \"".chars().count());
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
    fn fullscreen_editor_identifier_cases_come_from_current_buffer_first() {
        let lines = vec!["10 NewVar=1".to_string(), "20 newvar=2".to_string()];
        let editor = BasicEditor::new(&lines);
        let fallback = HashMap::from([("NEWVAR".to_string(), "newvar".to_string())]);

        let cases = editor_identifier_cases(&editor.lines, Some(&fallback));

        assert_eq!(cases.get("NEWVAR"), Some(&"NewVar".to_string()));
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

    #[test]
    fn status_bar_styles_help_keys_without_changing_width() {
        let plain =
            "F12 Apply Esc Cancel F3/F4 Undo/Redo F9 Renum Tab/Shift+Tab Inspect Ln 1 Col 1";
        let styled = style_editor_status_keys(plain);

        assert_eq!(visible_width(&styled), plain.chars().count());
        assert!(styled.contains(&format!("{STATUS_KEY_STYLE}F12{STATUS_KEY_END_STYLE}")));
        assert!(styled.contains(&format!("{STATUS_KEY_STYLE}F3/F4{STATUS_KEY_END_STYLE}")));
        assert!(styled.contains(&format!("{STATUS_KEY_STYLE}F9{STATUS_KEY_END_STYLE}")));
        assert!(styled.contains(&format!(
            "{STATUS_KEY_STYLE}Esc{STATUS_KEY_END_STYLE} Cancel"
        )));
        assert!(styled.contains(&format!(
            "{STATUS_KEY_STYLE}Tab/Shift+Tab{STATUS_KEY_END_STYLE} Inspect"
        )));
    }

    fn sample_debug_snapshot() -> crate::debugger::DebugSnapshot {
        use crate::debugger::{
            DebugArrayKind, DebugArraySummary, DebugDataSnapshot, DebugFrameKind, DebugLocation,
            DebugPauseReason, DebugSnapshot, DebugStackFrame, DebugTimerSnapshot, DebugValue,
            DebugVariable,
        };
        DebugSnapshot {
            reason: DebugPauseReason::Breakpoint,
            location: DebugLocation {
                line: 20,
                statement: 1,
                source_span: None,
                source: String::from("20 CALL DRAW(I):PRINT I"),
                command: String::from("PRINT I"),
            },
            source_lines: vec![
                String::from("10 A=1"),
                String::from("20 CALL DRAW(I):PRINT I"),
            ],
            variables: vec![
                DebugVariable {
                    name: String::from("I"),
                    value: DebugValue::Number(4.0),
                },
                DebugVariable {
                    name: String::from("NAME$"),
                    value: DebugValue::String(String::from("SHIP")),
                },
            ],
            array_elements: vec![DebugVariable {
                name: String::from("P(7,3)"),
                value: DebugValue::Number(19.0),
            }],
            arrays: vec![DebugArraySummary {
                name: String::from("P"),
                alias_of: Some(String::from("A")),
                kind: DebugArrayKind::Number,
                dimensions: vec![20, 10],
                elements: 231,
            }],
            stack: vec![
                DebugStackFrame {
                    kind: DebugFrameKind::Program,
                    name: None,
                    line: Some(10),
                },
                DebugStackFrame {
                    kind: DebugFrameKind::Sub,
                    name: Some(String::from("DRAW")),
                    line: Some(20),
                },
                DebugStackFrame {
                    kind: DebugFrameKind::Mouse,
                    name: None,
                    line: Some(900),
                },
            ],
            err: 5,
            erl: 120,
            timers: vec![DebugTimerSnapshot {
                number: 1,
                target: 500,
                repeat: true,
                active: true,
                interval: Duration::from_millis(1000),
                remaining: Duration::from_millis(250),
            }],
            data: DebugDataSnapshot::Next {
                position: 4,
                line: 100,
                line_item: 3,
                value: DebugValue::String(String::from("READY")),
            },
        }
    }

    #[test]
    fn debugger_uses_contiguous_function_keys_without_f10_or_f11() {
        use crate::debugger::DebugAction;
        assert_eq!(
            debug_action_for_key(KeyCode::F(1), KeyModifiers::NONE),
            Some(DebugAction::Continue)
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(5), KeyModifiers::NONE),
            Some(DebugAction::Continue)
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(6), KeyModifiers::NONE),
            Some(DebugAction::StepInto)
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(7), KeyModifiers::NONE),
            Some(DebugAction::StepOver)
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(8), KeyModifiers::NONE),
            Some(DebugAction::StepOut)
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(10), KeyModifiers::NONE),
            None
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(11), KeyModifiers::NONE),
            None
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(11), KeyModifiers::CONTROL),
            None
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(11), KeyModifiers::SHIFT),
            None
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(11), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            None
        );
        assert_eq!(
            debug_action_for_key(KeyCode::Esc, KeyModifiers::NONE),
            Some(DebugAction::Abort)
        );
        assert_eq!(
            debug_action_for_key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(DebugAction::Abort)
        );
        assert_eq!(
            debug_action_for_key(
                KeyCode::Char('C'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            ),
            Some(DebugAction::Abort)
        );
        assert_eq!(
            debug_action_for_key(KeyCode::F(9), KeyModifiers::NONE),
            None
        );
        let help = BasicEditor::default_help();
        assert!(help.contains("F5/F6 Copy/Paste"));
        assert!(help.contains("F9 Renum"));
        assert!(!help.contains("Break"));
        assert!(!help.contains("Debug"));
        assert!(!help.contains("Step"));
    }

    #[test]
    fn editor_breakpoints_toggle_by_basic_line_and_session_prunes_stale_entries() {
        let lines = vec!["10 PRINT 1".to_string(), "20 END".to_string()];
        let mut editor = BasicEditor::new(&lines);
        assert_eq!(editor.toggle_breakpoint(), Ok((true, 10)));
        assert!(editor.breakpoints.contains(&10));
        assert_eq!(editor.toggle_breakpoint(), Ok((false, 10)));

        editor.cursor_line = 1;
        assert_eq!(editor.toggle_breakpoint(), Ok((true, 20)));
        editor.breakpoints.insert(999);
        assert_eq!(editor.session().breakpoints(), &HashSet::from([20]));

        editor.lines[1] = "REM not a numbered line".chars().collect();
        assert_eq!(editor.toggle_breakpoint(), Err("no BASIC line"));
    }

    #[test]
    fn renum_moves_breakpoints_and_undo_restores_them() {
        let lines = vec![
            "100 GOTO 295".to_string(),
            "295 PRINT 1".to_string(),
            "330 RETURN".to_string(),
        ];
        let mut editor = BasicEditor::with_breakpoints(&lines, HashSet::from([295, 330]));

        editor.renumber_visible_lines().unwrap();
        assert_eq!(editor.breakpoints, HashSet::from([135, 170]));
        assert!(editor.undo());
        assert_eq!(editor.breakpoints, HashSet::from([295, 330]));
    }

    #[test]
    fn debugger_gutter_distinguishes_breakpoint_execution_and_viewed_lines() {
        assert_eq!(render_editor_gutter(false, false, false, false), "   ");
        assert_eq!(render_editor_gutter(false, true, false, false), "●  ");
        assert_eq!(render_editor_gutter(false, false, true, false), "  ●");
        assert_eq!(render_editor_gutter(false, true, true, false), "● ●");
        assert_eq!(render_editor_gutter(false, false, false, true), "  ▶");
        assert_eq!(render_editor_gutter(false, true, false, true), "● ▶");
        assert_eq!(render_editor_gutter(false, true, true, true), "● ●");
        assert!(render_editor_gutter(true, false, false, true)
            .contains(&format!("{DEBUG_BREAKPOINT_STYLE}▶{RESET}")));
        assert_eq!(
            render_editor_gutter_for_theme(true, false, true, false, SyntaxTheme::Dark),
            format!("  {DEBUG_EXECUTION_DOT_DARK_STYLE}●{RESET}")
        );
        assert_eq!(
            render_editor_gutter_for_theme(true, false, true, false, SyntaxTheme::Light),
            format!("  {DEBUG_EXECUTION_DOT_LIGHT_STYLE}●{RESET}")
        );
        assert_eq!(
            render_editor_gutter_for_theme(true, true, true, true, SyntaxTheme::Light),
            format!("{DEBUG_BREAKPOINT_STYLE}●{RESET} {DEBUG_EXECUTION_DOT_LIGHT_STYLE}●{RESET}")
        );
        assert_eq!(
            visible_width(&render_editor_gutter(true, true, true, true)),
            EDITOR_GUTTER_WIDTH
        );
    }

    #[test]
    fn debugger_places_the_execution_marker_before_the_exact_statement() {
        let line = "915 P=N : GOSUB 700 : GOSUB 700"
            .chars()
            .collect::<Vec<_>>();

        assert_eq!(debug_statement_column(&line, 0, None), Some(4));
        assert_eq!(debug_statement_column(&line, 1, None), Some(10));
        assert_eq!(debug_statement_column(&line, 2, None), Some(22));

        let (visible, marker_col) = debug_source_window(&line, 0, 40, 1, None);
        assert_eq!(marker_col, Some(10));
        let rendered = finish_debug_statement_marker(&visible, false, marker_col.unwrap());
        assert_eq!(rendered, "915 P=N : ▶ GOSUB 700 : GOSUB 700");

        let (visible, marker_col) = debug_source_window(&line, 11, 12, 1, None);
        assert_eq!(marker_col, None);
        assert_eq!(visible.chars().count(), 12);
        assert!(!visible.contains(DEBUG_STATEMENT_PLACEHOLDER));

        let (visible, marker_col) = debug_source_window(&line, 8, 20, 2, None);
        assert_eq!(marker_col, Some(14));
        let rendered = finish_debug_statement_marker(&visible, false, marker_col.unwrap());
        assert_eq!(rendered.chars().nth(14), Some('▶'));
    }

    #[test]
    fn debugger_source_spans_distinguish_repeated_then_and_else_statements() {
        let source = "10 IF X THEN A=1:A=1 ELSE A=1:A=1";
        let line = source.chars().collect::<Vec<_>>();
        let starts = source
            .match_indices("A=1")
            .map(|(start, _)| start)
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 4);

        for start in &starts {
            let span = *start..*start + "A=1".len();
            assert_eq!(
                debug_statement_column(&line, usize::MAX, Some(&span)),
                Some(source[..*start].chars().count())
            );
        }

        let then_span = starts[0]..starts[0] + "A=1".len();
        let (visible, marker_col) = debug_source_window(&line, 0, 80, usize::MAX, Some(&then_span));
        let rendered = finish_debug_statement_marker(&visible, false, marker_col.unwrap());
        assert_eq!(rendered, "10 IF X THEN ▶ A=1:A=1 ELSE A=1:A=1");

        let else_span = starts[2]..starts[2] + "A=1".len();
        let (visible, marker_col) = debug_source_window(&line, 0, 80, usize::MAX, Some(&else_span));
        let rendered = finish_debug_statement_marker(&visible, false, marker_col.unwrap());
        assert_eq!(rendered, "10 IF X THEN A=1:A=1 ELSE ▶ A=1:A=1");
    }

    #[test]
    fn debugger_source_span_byte_offsets_become_utf8_character_columns() {
        let source = "10 IF 1 THEN PRINT \"é:ELSE\":B=2 ELSE B=2";
        let line = source.chars().collect::<Vec<_>>();
        let start = source.find("B=2").unwrap();
        let span = start..start + "B=2".len();
        let expected_column = source[..start].chars().count();

        assert!(start > expected_column);
        assert_eq!(
            debug_statement_column(&line, usize::MAX, Some(&span)),
            Some(expected_column)
        );
        let (visible, marker_col) = debug_source_window(&line, 0, 80, usize::MAX, Some(&span));
        let rendered = finish_debug_statement_marker(&visible, false, marker_col.unwrap());
        assert_eq!(rendered, "10 IF 1 THEN PRINT \"é:ELSE\":▶ B=2 ELSE B=2");
    }

    #[test]
    fn debugger_invalid_source_spans_safely_fall_back_to_the_statement_index() {
        let source = "10 PRINT \"é\":B=2";
        let line = source.chars().collect::<Vec<_>>();
        let fallback = debug_statement_column(&line, 1, None);
        let command_start = source.find("B=2").unwrap();
        assert_eq!(fallback, Some(source[..command_start].chars().count()));

        let inside_utf8 = source.find('é').unwrap() + 1;
        let invalid_utf8 = inside_utf8..inside_utf8 + 1;
        assert_eq!(
            debug_statement_column(&line, 1, Some(&invalid_utf8)),
            fallback
        );

        let out_of_bounds = source.len()..source.len() + 1;
        assert_eq!(
            debug_statement_column(&line, 1, Some(&out_of_bounds)),
            fallback
        );

        let (start, end) = (8, 3);
        let reversed = start..end;
        assert_eq!(debug_statement_column(&line, 1, Some(&reversed)), fallback);
    }

    #[test]
    fn debugger_reveals_a_spanned_inline_statement_when_horizontally_scrolled() {
        let source = format!("10 IF 1 THEN PRINT \"{}\":TARGET=1", "X".repeat(60));
        let target_start = source.find("TARGET=1").unwrap();
        let target_span = target_start..target_start + "TARGET=1".len();
        let mut editor = BasicEditor::new(std::slice::from_ref(&source));

        let statement_col =
            reveal_debug_statement(&mut editor, 0, usize::MAX, Some(&target_span), 20, true)
                .unwrap();
        assert_eq!(statement_col, source[..target_start].chars().count());
        assert!(editor.left_col > 0);

        let (visible, marker_col) = debug_source_window(
            &editor.lines[0],
            editor.left_col,
            20,
            usize::MAX,
            Some(&target_span),
        );
        let rendered = finish_debug_statement_marker(&visible, false, marker_col.unwrap());
        assert!(rendered.contains("▶ TARGET=1"));
    }

    #[test]
    fn debugger_statement_marker_preserves_ansi_width_and_execution_background() {
        let line = "915 P=N : GOSUB 700".chars().collect::<Vec<_>>();
        let (visible, marker_col) = debug_source_window(&line, 0, 20, 1, None);
        let marker_col = marker_col.unwrap();
        let highlighted = syntax_highlight_raw_with_cases(&visible, true, None);
        let marked = finish_debug_statement_marker(&highlighted, true, marker_col);
        let rendered = apply_debug_execution_line_style(&marked, true, 20);

        assert!(rendered.contains(&format!(
            "{}▶{RESET}",
            debug_execution_arrow_style(current_syntax_theme())
        )));
        assert!(!rendered.contains(DEBUG_STATEMENT_PLACEHOLDER));
        assert_eq!(visible_width(&rendered), 20);
    }

    #[test]
    fn debugger_statement_marker_uses_an_independent_arrow_style_for_each_theme() {
        let rendered = format!("A{DEBUG_STATEMENT_PLACEHOLDER} B");
        let dark = finish_debug_statement_marker_for_theme(&rendered, true, 1, SyntaxTheme::Dark);
        let light = finish_debug_statement_marker_for_theme(&rendered, true, 1, SyntaxTheme::Light);

        assert!(dark.contains(&format!("{DEBUG_EXECUTION_ARROW_DARK_STYLE}▶{RESET}")));
        assert!(light.contains(&format!("{DEBUG_EXECUTION_ARROW_LIGHT_STYLE}▶{RESET}")));
        assert!(!dark.contains(DEBUG_EXECUTION_DOT_DARK_STYLE));
        assert!(!light.contains(DEBUG_EXECUTION_DOT_LIGHT_STYLE));
        assert_eq!(visible_width(&dark), visible_width(&light));
    }

    #[test]
    fn debugger_replaces_only_its_own_statement_placeholder() {
        let line = format!("10 PRINT \"{DEBUG_STATEMENT_PLACEHOLDER}\" : END")
            .chars()
            .collect::<Vec<_>>();
        let (visible, marker_col) = debug_source_window(&line, 0, 80, 1, None);
        let rendered = finish_debug_statement_marker(&visible, false, marker_col.unwrap());

        assert_eq!(rendered.matches('▶').count(), 1);
        assert_eq!(rendered.matches(DEBUG_STATEMENT_PLACEHOLDER).count(), 1);
        assert!(rendered.ends_with("▶ END"));
    }

    #[test]
    fn debugger_execution_background_survives_syntax_resets() {
        let highlighted = format!("{LINE_NUMBER_STYLE}10{RESET} {KEYWORD_STYLE}PRINT{RESET} A");
        let rendered = apply_debug_execution_line_style(&highlighted, true, 20);
        let background = match current_syntax_theme() {
            SyntaxTheme::Dark => DEBUG_CURRENT_LINE_DARK_STYLE,
            SyntaxTheme::Light => DEBUG_CURRENT_LINE_LIGHT_STYLE,
        };

        assert!(rendered.starts_with(background));
        assert!(rendered.contains(&format!("{RESET}{background}")));
        assert!(rendered.ends_with(RESET));
        assert_eq!(visible_width(&rendered), 20);
    }

    #[test]
    fn debugger_string_values_escape_every_terminal_control() {
        use crate::debugger::DebugValue;
        let value = DebugValue::String(String::from("A\"\\\0\t\n\r\x08\x1b\x7f\u{85}Z"));
        let rendered = format_debug_value(&value);

        assert_eq!(rendered, "\"A\"\"\\\\\\0\\t\\n\\r\\x08\\x1B\\x7F\\x85Z\"");
        assert!(!rendered.chars().any(char::is_control));
    }

    #[test]
    fn fullscreen_session_round_trip_preserves_complete_editor_state() {
        let lines = vec!["10 PRINT 1".to_string(), "20 END".to_string()];
        let mut editor = BasicEditor::with_breakpoints(&lines, HashSet::from([20]));
        editor.cursor_line = 1;
        editor.cursor_col = 2;
        editor.top_line = 1;
        editor.left_col = 1;
        editor.page_rows = 7;
        editor.selection_anchor = Some(EditorPosition { line: 0, col: 3 });
        editor.clipboard = String::from("PRINT");
        editor.last_find = String::from("END");
        editor.last_replace = String::from("STOP");
        editor.insert_text(" REM");
        assert!(editor.undo());

        let expected = editor.clone();
        let session = editor.session();
        assert_eq!(session.lines(), lines);
        assert_eq!(session.breakpoints(), &HashSet::from([20]));

        let restored = session.into_editor();
        assert_eq!(restored.cursor_line, expected.cursor_line);
        assert_eq!(restored.cursor_col, expected.cursor_col);
        assert_eq!(restored.top_line, expected.top_line);
        assert_eq!(restored.left_col, expected.left_col);
        assert_eq!(restored.selection_anchor, expected.selection_anchor);
        assert_eq!(restored.clipboard, expected.clipboard);
        assert_eq!(restored.last_find, expected.last_find);
        assert_eq!(restored.last_replace, expected.last_replace);
        assert_eq!(restored.undo_stack, expected.undo_stack);
        assert_eq!(restored.redo_stack, expected.redo_stack);
        assert!(!restored.dirty);
        assert_eq!(restored.breakpoints, HashSet::from([20]));
    }

    #[test]
    fn debugger_layout_hides_inspector_only_on_very_short_terminals() {
        let wide = debug_layout(120, 30);
        assert_eq!(wide.panel_placement, DebugPanelPlacement::Right);
        assert_eq!(wide.gutter_width, EDITOR_GUTTER_WIDTH);
        assert_eq!(wide.code_rows, 29);
        assert_eq!(wide.panel_rows, 29);
        assert_eq!(wide.code_region_cols + 1 + wide.panel_cols, 120);
        assert_eq!(wide.panel_x, wide.code_region_cols + 1);

        let normal = debug_layout(100, 30);
        assert_eq!(normal.panel_placement, DebugPanelPlacement::Below);
        assert!(normal.code_rows > normal.panel_rows);
        assert!(normal.panel_rows >= 4);
        assert_eq!(normal.code_rows + normal.panel_rows, 29);
        assert_eq!(normal.panel_y, normal.code_rows);

        let short = debug_layout(40, 7);
        assert_eq!(short.panel_placement, DebugPanelPlacement::Hidden);
        assert_eq!(short.panel_rows, 0);
        assert_eq!(short.status_row, 6);
        assert_eq!(short.code_rows, 6);
    }

    #[test]
    fn debugger_status_starts_with_pause_reason_and_only_advertises_visible_inspector() {
        let snapshot = sample_debug_snapshot();
        let with_panel = debug_status(&snapshot, Some(10), true);
        assert!(with_panel.starts_with("BREAKPOINT | Ln 20 Stmt 2 View 10 |"));
        assert!(with_panel.contains("F5 Go F6 Into F7 Over F8 Out"));
        assert!(!with_panel.contains("F10"));
        assert!(!with_panel.contains("F11"));
        assert!(with_panel.contains("Tab/Shift+Tab Inspect"));

        let without_panel = debug_status(&snapshot, Some(10), false);
        assert!(without_panel.starts_with("BREAKPOINT | Ln 20 Stmt 2 View 10 |"));
        assert!(!without_panel.contains("Tab/Shift+Tab Inspect"));
        assert_eq!(fit_plain_text(&without_panel, 20), "BREAKPOINT | Ln 20 S");
        assert!(
            debug_status_message(&snapshot, Some(10), "Breakpoint set at 10")
                .starts_with("BREAKPOINT | Ln 20 Stmt 2 View 10 |")
        );
    }

    #[test]
    fn debugger_keeps_alternate_screen_only_for_step_actions() {
        use crate::debugger::DebugAction;

        assert!(!debug_action_keeps_alternate_screen(DebugAction::Continue));
        assert!(debug_action_keeps_alternate_screen(DebugAction::StepInto));
        assert!(debug_action_keeps_alternate_screen(DebugAction::StepOver));
        assert!(debug_action_keeps_alternate_screen(DebugAction::StepOut));
        assert!(!debug_action_keeps_alternate_screen(DebugAction::Abort));
    }

    #[test]
    fn debugger_terminal_retains_silent_steps_and_reveals_runtime_io_once() {
        use crate::debugger::DebugAction;
        use DebugTerminalOperation::*;

        let (terminal, operations) = DebugTerminalSession::recording();

        terminal
            .enter_pause()
            .unwrap()
            .finish(DebugAction::StepInto)
            .unwrap();
        terminal
            .enter_pause()
            .unwrap()
            .finish(DebugAction::StepOver)
            .unwrap();
        assert!(terminal.reveal_runtime_console().unwrap());
        assert!(!terminal.reveal_runtime_console().unwrap());
        terminal
            .enter_pause()
            .unwrap()
            .finish(DebugAction::Continue)
            .unwrap();

        assert_eq!(
            operations.borrow().as_slice(),
            [
                EnableRawMode,
                EnterAlternateScreen,
                ShowCursor,
                DisableRawMode,
                EnableRawMode,
                ShowCursor,
                DisableRawMode,
                ShowCursor,
                LeaveAlternateScreen,
                EnableRawMode,
                EnterAlternateScreen,
                ShowCursor,
                LeaveAlternateScreen,
                DisableRawMode,
            ]
        );
    }

    #[test]
    fn debugger_terminal_guard_restores_primary_screen_on_early_exit() {
        use DebugTerminalOperation::*;

        let (terminal, operations) = DebugTerminalSession::recording();
        let guard = terminal.enter_pause().unwrap();
        drop(guard);

        assert_eq!(
            operations.borrow().as_slice(),
            [
                EnableRawMode,
                EnterAlternateScreen,
                ShowCursor,
                LeaveAlternateScreen,
                DisableRawMode,
            ]
        );
        assert!(!terminal.inner.alternate_screen.get());
        assert!(!terminal.inner.debugger_raw_mode.get());
    }

    #[test]
    fn debugger_terminal_session_restores_an_idle_alternate_screen_on_drop() {
        use crate::debugger::DebugAction;
        use DebugTerminalOperation::*;

        let (terminal, operations) = DebugTerminalSession::recording();
        terminal
            .enter_pause()
            .unwrap()
            .finish(DebugAction::StepOut)
            .unwrap();
        assert!(terminal.inner.alternate_screen.get());
        assert!(!terminal.inner.debugger_raw_mode.get());

        drop(terminal);
        assert_eq!(
            operations.borrow().as_slice(),
            [
                EnableRawMode,
                EnterAlternateScreen,
                ShowCursor,
                DisableRawMode,
                ShowCursor,
                LeaveAlternateScreen,
            ]
        );
    }

    #[test]
    fn debugger_panel_scroll_clamps_after_responsive_resize() {
        let snapshot = sample_debug_snapshot();
        let narrow = debug_layout(42, 24);
        let scrolled = clamp_debug_panel_scroll(&snapshot, narrow, usize::MAX);
        assert!(scrolled > 0);

        let wide = debug_layout(120, 24);
        let clamped = clamp_debug_panel_scroll(&snapshot, wide, scrolled);
        let wide_lines = debug_panel_lines(&snapshot, wide.panel_cols);
        assert!(clamped <= wide_lines.len().saturating_sub(wide.panel_rows));
    }

    #[test]
    fn debugger_panel_tab_reaches_the_last_page_then_wraps() {
        let snapshot = sample_debug_snapshot();
        let layout = debug_layout(42, 24);
        let lines = debug_panel_lines(&snapshot, layout.panel_cols);
        let max_scroll = lines.len().saturating_sub(layout.panel_rows);
        assert!(max_scroll > 0);

        assert_eq!(
            advance_debug_panel_scroll(&snapshot, layout, max_scroll, false),
            0
        );
        assert_eq!(
            advance_debug_panel_scroll(&snapshot, layout, max_scroll, true),
            max_scroll.saturating_sub(debug_panel_page_size())
        );
    }

    #[test]
    fn debugger_horizontal_scroll_is_read_only_and_bounded() {
        let lines = vec![format!("10 PRINT {}", "X".repeat(80))];
        let mut editor = BasicEditor::new(&lines);
        let original = editor.lines_as_strings();

        let view_line_len = editor.current_line_len();
        editor.scroll_debug_right(20, view_line_len);
        assert_eq!(editor.left_col, 4);
        editor.scroll_debug_left();
        assert_eq!(editor.left_col, 0);
        assert_eq!(editor.lines_as_strings(), original);
        assert!(!editor.dirty);
        assert!(editor.undo_stack.is_empty());
    }

    #[test]
    fn debugger_horizontal_scroll_accounts_for_the_inline_marker_width() {
        let lines = vec![format!("10 A=1 : PRINT \"{}\"", "X".repeat(80))];
        let mut editor = BasicEditor::new(&lines);
        let display_len = editor.current_line_len() + 2;
        for _ in 0..100 {
            editor.scroll_debug_right(20, display_len);
        }
        assert_eq!(editor.left_col, display_len - 20);

        let (visible, marker_col) =
            debug_source_window(&editor.lines[0], editor.left_col, 20, 1, None);
        assert_eq!(marker_col, None);
        assert_eq!(visible.chars().count(), 20);
        assert!(visible.ends_with('"'));
    }

    #[test]
    fn debugger_panel_adapts_and_contains_all_snapshot_categories() {
        let snapshot = sample_debug_snapshot();
        let wide = debug_panel_lines(&snapshot, 120);
        assert!(wide.iter().all(|line| line.chars().count() == 120));
        let wide_text = wide.join("\n");
        for expected in [
            "VARIABLES",
            "I = 4",
            "NAME$ = \"SHIP\"",
            "P(7,3) = 19",
            "ARRAYS",
            "P -> A(20,10)",
            "STACK",
            "SUB DRAW @20",
            "MOUSE @900",
            "ERR=5 ERL=120",
            "TIMERS",
            "#1 EVERY -> 500 active 250/1000ms",
            "DATA",
            "Ln 100 Item 3: \"READY\"",
        ] {
            assert!(
                wide_text.contains(expected),
                "missing {expected:?}: {wide_text}"
            );
        }

        let narrow = debug_panel_lines(&snapshot, 42);
        assert!(narrow.iter().all(|line| line.chars().count() == 42));
        assert!(narrow.len() > wide.len());
        let narrow_text = narrow.join("\n");
        assert!(narrow_text.contains("VARIABLES"));
        assert!(narrow_text.contains("DATA"));
        assert!(!narrow_text.contains("LOCATION"));
        assert!(!narrow_text.contains("PRINT I"));
    }

    #[test]
    fn debugger_data_marks_visible_changes_and_distinguishes_empty_from_end() {
        use crate::debugger::{DebugDataSnapshot, DebugValue};

        let snapshot = sample_debug_snapshot();
        let mut history = DebugInspectionHistory::default();
        history.remember(&snapshot);

        assert!(!history.changes(&snapshot).data);

        let mut changed_at_same_position = snapshot.clone();
        changed_at_same_position.data = DebugDataSnapshot::Next {
            position: 4,
            line: 999,
            line_item: 99,
            value: DebugValue::Number(123.0),
        };
        assert!(
            history.changes(&changed_at_same_position).data,
            "a change in any displayed DATA field must be highlighted"
        );

        let mut duplicate_value_at_next_position = snapshot.clone();
        duplicate_value_at_next_position.data = DebugDataSnapshot::Next {
            position: 5,
            line: 100,
            line_item: 4,
            value: DebugValue::String(String::from("READY")),
        };
        assert!(
            history.changes(&duplicate_value_at_next_position).data,
            "advancing over duplicate DATA values must still be highlighted"
        );

        let mut nan = snapshot.clone();
        nan.data = DebugDataSnapshot::Next {
            position: 4,
            line: 100,
            line_item: 3,
            value: DebugValue::Number(f64::NAN),
        };
        history.remember(&nan);
        assert!(
            !history.changes(&nan).data,
            "an unchanged displayed NaN must not flash on every pause"
        );

        let mut negative_zero = nan.clone();
        negative_zero.data = DebugDataSnapshot::Next {
            position: 4,
            line: 100,
            line_item: 3,
            value: DebugValue::Number(-0.0),
        };
        history.remember(&negative_zero);
        let mut positive_zero = negative_zero.clone();
        positive_zero.data = DebugDataSnapshot::Next {
            position: 4,
            line: 100,
            line_item: 3,
            value: DebugValue::Number(0.0),
        };
        assert!(
            history.changes(&positive_zero).data,
            "values displayed as -0 and 0 must be distinguished"
        );

        let mut exhausted = snapshot.clone();
        exhausted.data = DebugDataSnapshot::Exhausted {
            position: 5,
            total: 5,
        };
        let changes = history.changes(&exhausted);
        assert!(changes.data);
        let exhausted_text = debug_panel_lines_with_changes(&exhausted, &changes, 120).join("\n");
        assert!(exhausted_text.contains("* (end)"));

        let mut empty = snapshot;
        empty.data = DebugDataSnapshot::Empty;
        let empty_text = debug_panel_lines(&empty, 120).join("\n");
        assert!(empty_text.contains("DATA"));
        assert!(empty_text.contains("  (none)"));
    }

    #[test]
    fn debugger_panel_styles_headings_per_cell_without_leaking_into_values() {
        let width = 60;
        let column_width = debug_panel_column_width(width, 2);
        let plain = fit_plain_text(
            &format!(
                "{} | {}",
                fit_plain_text("ARRAYS", column_width),
                fit_plain_text("  DATABASE = 1", column_width)
            ),
            width,
        );
        let styled = style_debug_panel_line_for_theme(&plain, width, true, SyntaxTheme::Dark);

        assert_eq!(styled.matches(DEBUG_PANEL_HEADER_STYLE).count(), 1);
        assert_eq!(visible_width(&styled), width);
        let (heading, value) = styled.split_once(" | ").unwrap();
        assert!(heading.starts_with(DEBUG_PANEL_HEADER_STYLE));
        assert!(heading.ends_with(RESET));
        assert!(!value.contains(DEBUG_PANEL_HEADER_STYLE));
        assert!(value.contains("DATABASE = 1"));
    }

    #[test]
    fn debugger_panel_colors_only_changed_cells_and_keeps_plain_fallback() {
        let snapshot = sample_debug_snapshot();
        let changes = DebugPanelChanges {
            variables: HashSet::from([String::from("I")]),
            ..DebugPanelChanges::default()
        };
        let width = 120;
        let plain = debug_panel_lines_with_changes(&snapshot, &changes, width)
            .into_iter()
            .find(|line| line.contains("* I = 4"))
            .unwrap();

        let fallback = style_debug_panel_line_for_theme(&plain, width, false, SyntaxTheme::Dark);
        assert_eq!(fallback, plain);
        assert!(!fallback.contains('\x1b'));

        let dark = style_debug_panel_line_for_theme(&plain, width, true, SyntaxTheme::Dark);
        let light = style_debug_panel_line_for_theme(&plain, width, true, SyntaxTheme::Light);
        assert!(dark.contains(&format!("{DEBUG_PANEL_CHANGED_DARK_STYLE}* I = 4")));
        assert!(light.contains(&format!("{DEBUG_PANEL_CHANGED_LIGHT_STYLE}* I = 4")));
        assert!(!dark.contains(DEBUG_PANEL_CHANGED_LIGHT_STYLE));
        assert!(!light.contains(DEBUG_PANEL_CHANGED_DARK_STYLE));
        assert_eq!(dark.matches(DEBUG_PANEL_CHANGED_DARK_STYLE).count(), 1);
        assert_eq!(light.matches(DEBUG_PANEL_CHANGED_LIGHT_STYLE).count(), 1);
        assert_eq!(visible_width(&dark), width);
        assert_eq!(visible_width(&light), width);

        let first_gap = dark.find(" | ").unwrap();
        let first_reset = dark.find(RESET).unwrap();
        assert!(
            first_reset < first_gap,
            "changed style leaked into the next cell"
        );
        assert!(!dark[first_gap..].contains(DEBUG_PANEL_CHANGED_DARK_STYLE));
    }

    #[test]
    fn debugger_panel_colors_changed_cells_in_second_and_third_columns() {
        let snapshot = sample_debug_snapshot();

        let state_changes = DebugPanelChanges {
            state: true,
            ..DebugPanelChanges::default()
        };
        let two_column_width = 60;
        let state_plain =
            debug_panel_lines_with_changes(&snapshot, &state_changes, two_column_width)
                .into_iter()
                .find(|line| line.contains("* ERR=5 ERL=120"))
                .unwrap();
        let state_styled = style_debug_panel_line_for_theme(
            &state_plain,
            two_column_width,
            true,
            SyntaxTheme::Dark,
        );
        let first_gap = state_styled.find(" | ").unwrap();
        let state_style = state_styled.find(DEBUG_PANEL_CHANGED_DARK_STYLE).unwrap();
        assert!(state_style > first_gap);
        assert_eq!(
            state_styled.matches(DEBUG_PANEL_CHANGED_DARK_STYLE).count(),
            1
        );
        assert_eq!(visible_width(&state_styled), two_column_width);

        let timer_changes = DebugPanelChanges {
            timers: HashSet::from([1]),
            ..DebugPanelChanges::default()
        };
        let three_column_width = 120;
        let timer_plain =
            debug_panel_lines_with_changes(&snapshot, &timer_changes, three_column_width)
                .into_iter()
                .find(|line| line.contains("* #1 EVERY"))
                .unwrap();
        let timer_styled = style_debug_panel_line_for_theme(
            &timer_plain,
            three_column_width,
            true,
            SyntaxTheme::Dark,
        );
        let second_gap = timer_styled.rfind(" | ").unwrap();
        let timer_style = timer_styled.find(DEBUG_PANEL_CHANGED_DARK_STYLE).unwrap();
        assert!(timer_style > second_gap);
        assert_eq!(
            timer_styled.matches(DEBUG_PANEL_CHANGED_DARK_STYLE).count(),
            1
        );
        assert_eq!(visible_width(&timer_styled), three_column_width);
    }

    #[test]
    fn debugger_panel_change_style_ignores_separator_text_inside_values() {
        use crate::debugger::{DebugValue, DebugVariable};

        let mut snapshot = sample_debug_snapshot();
        snapshot.variables = vec![DebugVariable {
            name: String::from("NAME$"),
            value: DebugValue::String(String::from("LEFT | RIGHT")),
        }];
        snapshot.array_elements.clear();
        let changes = DebugPanelChanges {
            variables: HashSet::from([String::from("NAME$")]),
            ..DebugPanelChanges::default()
        };
        let width = 120;
        let plain = debug_panel_lines_with_changes(&snapshot, &changes, width)
            .into_iter()
            .find(|line| line.contains("* NAME$"))
            .unwrap();
        let styled = style_debug_panel_line_for_theme(&plain, width, true, SyntaxTheme::Dark);

        assert!(styled.contains(&format!(
            "{DEBUG_PANEL_CHANGED_DARK_STYLE}* NAME$ = \"LEFT | RIGHT\""
        )));
        assert_eq!(styled.matches(DEBUG_PANEL_CHANGED_DARK_STYLE).count(), 1);
        assert_eq!(visible_width(&styled), width);
    }

    #[test]
    fn debugger_panel_change_style_survives_single_column_truncation() {
        let fallback = style_debug_panel_line_for_theme(
            "* VERY_LONG_VARIABLE = 123",
            1,
            false,
            SyntaxTheme::Dark,
        );
        let styled = style_debug_panel_line_for_theme(
            "* VERY_LONG_VARIABLE = 123",
            1,
            true,
            SyntaxTheme::Dark,
        );

        assert_eq!(fallback, "*");
        assert_eq!(styled, format!("{DEBUG_PANEL_CHANGED_DARK_STYLE}*{RESET}"));
        assert_eq!(visible_width(&styled), 1);
    }

    #[test]
    fn debugger_inspector_marks_net_changes_since_the_previous_pause() {
        use crate::debugger::{DebugValue, DebugVariable};

        let snapshot = sample_debug_snapshot();
        let mut history = DebugInspectionHistory::default();
        assert_eq!(history.changes(&snapshot), DebugPanelChanges::default());
        history.remember(&snapshot);

        let mut countdown_only = snapshot.clone();
        countdown_only.timers[0].remaining = Duration::from_millis(10);
        assert_eq!(
            history.changes(&countdown_only),
            DebugPanelChanges::default(),
            "the natural timer countdown must not flash on every pause"
        );

        let mut changed = countdown_only;
        changed.variables[0].value = DebugValue::Number(5.0);
        changed.variables.push(DebugVariable {
            name: String::from("NEW$"),
            value: DebugValue::String(String::from("VISIBLE")),
        });
        changed.array_elements[0] = DebugVariable {
            name: String::from("P(12,3)"),
            value: DebugValue::Number(8.0),
        };
        changed.arrays[0].elements += 1;
        changed.err = 6;
        changed.timers[0].active = false;
        changed.data = crate::debugger::DebugDataSnapshot::Next {
            position: 5,
            line: 100,
            line_item: 4,
            value: DebugValue::Number(9.0),
        };

        let changes = history.changes(&changed);
        assert_eq!(
            changes.variables,
            HashSet::from([
                String::from("I"),
                String::from("NEW$"),
                String::from("P(12,3)"),
            ])
        );
        assert_eq!(changes.arrays, HashSet::from([String::from("P")]));
        assert!(changes.state);
        assert_eq!(changes.timers, HashSet::from([1]));
        assert!(changes.data);

        for width in [42, 120] {
            let lines = debug_panel_lines_with_changes(&changed, &changes, width);
            assert!(lines.iter().all(|line| line.chars().count() == width));
            let text = lines.join("\n");
            for expected in [
                "* I = 5",
                "* NEW$ = \"VISIBLE\"",
                "* P(12,3) = 8",
                "* P -> A",
                "* ERR=6",
                "* #1",
                "* Ln 100 Item 4: 9",
            ] {
                assert!(text.contains(expected), "missing {expected:?}: {text}");
            }
            assert!(text.contains("  NAME$ = \"SHIP\""));
        }
    }
}
