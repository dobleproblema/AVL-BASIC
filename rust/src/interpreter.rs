use crate::console;
use crate::error::{BasicError, BasicResult, ErrorCode};
use crate::expr::{
    call_pure_function, compile_expression, eval_compiled, eval_compiled_number, split_arguments,
    EvalContext, Expr,
};
use crate::fonts::FontKind;
use crate::graphics::{rgb_number, Graphics};
use crate::lexer::{split_commands, split_top_level, strip_comment};
use crate::program::Program;
use crate::value::{format_basic_number, round_half_away, Value};
use crate::window::{refocus_console_window, GraphicsWindow, MouseSnapshot};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const AVL_BASIC_LANGUAGE_VERSION: &str = "1.5.19";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    End,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Cursor {
    line_idx: usize,
    cmd_idx: usize,
}

#[derive(Debug, Clone)]
struct ForFrame {
    var: String,
    end: f64,
    step: f64,
    resume: Cursor,
}

#[derive(Debug, Clone)]
struct WhileFrame {
    expr: Rc<Expr>,
    header: Cursor,
    resume: Cursor,
}

#[derive(Debug, Clone)]
struct CompiledFor {
    var: String,
    start: Expr,
    end: Expr,
    step: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatOrientation {
    Normal,
    Row,
    Col,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IfBranchKind {
    ElseIf,
    Else,
    EndIf,
}

#[derive(Debug, Clone)]
struct IfBranch {
    cursor: Cursor,
    kind: IfBranchKind,
}

#[derive(Debug, Clone)]
enum MatExprValue {
    Scalar(Value),
    Matrix(ArrayValue),
}

#[derive(Debug, Clone)]
struct NumericMatrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

#[derive(Debug, Clone)]
struct BasicTimer {
    interval: Duration,
    next_fire: Instant,
    target: i32,
    repeat: bool,
    active: bool,
    pending: bool,
    pending_delay: u8,
    remaining_ticks: i32,
}

#[derive(Debug, Clone)]
struct ArrayValue {
    dims: Vec<usize>,
    data: ArrayData,
}

#[derive(Debug, Clone)]
enum ArrayData {
    Number(Vec<f64>),
    Str(Vec<String>),
}

impl ArrayValue {
    fn new(name: &str, dims: Vec<usize>) -> Self {
        let len = dims.iter().map(|d| d + 1).product();
        let data = if name.ends_with('$') {
            ArrayData::Str(vec![String::new(); len])
        } else {
            ArrayData::Number(vec![0.0; len])
        };
        Self { dims, data }
    }

    fn flat_index(&self, indexes: &[i32]) -> BasicResult<usize> {
        if indexes.len() != self.dims.len() {
            return Err(BasicError::new(ErrorCode::InvalidIndex));
        }
        let mut multiplier = 1usize;
        let mut flat = 0usize;
        for (idx, bound) in indexes.iter().rev().zip(self.dims.iter().rev()) {
            if *idx < 0 || *idx as usize > *bound {
                return Err(BasicError::new(ErrorCode::IndexOutOfRange));
            }
            flat += *idx as usize * multiplier;
            multiplier *= bound + 1;
        }
        Ok(flat)
    }

    fn is_string(&self) -> bool {
        matches!(self.data, ArrayData::Str(_))
    }

    fn fill(&mut self, value: Value) -> BasicResult<()> {
        match (&mut self.data, value) {
            (ArrayData::Number(values), Value::Number(n)) => {
                values.fill(n);
                Ok(())
            }
            (ArrayData::Str(values), Value::Str(s)) => {
                values.fill(s);
                Ok(())
            }
            _ => Err(BasicError::new(ErrorCode::TypeMismatch)),
        }
    }

    fn data_len(&self) -> usize {
        match &self.data {
            ArrayData::Number(values) => values.len(),
            ArrayData::Str(values) => values.len(),
        }
    }

    fn get(&self, indexes: &[i32]) -> BasicResult<Value> {
        let idx = self.flat_index(indexes)?;
        Ok(match &self.data {
            ArrayData::Number(values) => Value::number(values[idx]),
            ArrayData::Str(values) => Value::string(values[idx].clone()),
        })
    }

    fn get_direct_1d(&self, index: i32) -> BasicResult<Value> {
        if self.dims.len() != 1 {
            return Err(BasicError::new(ErrorCode::InvalidIndex));
        }
        if index < 0 || index as usize > self.dims[0] {
            return Err(BasicError::new(ErrorCode::IndexOutOfRange));
        }
        Ok(match &self.data {
            ArrayData::Number(values) => Value::number(values[index as usize]),
            ArrayData::Str(values) => Value::string(values[index as usize].clone()),
        })
    }

    fn set(&mut self, indexes: &[i32], value: Value) -> BasicResult<()> {
        let idx = self.flat_index(indexes)?;
        match (&mut self.data, value) {
            (ArrayData::Number(values), Value::Number(n)) => {
                values[idx] = n;
                Ok(())
            }
            (ArrayData::Str(values), Value::Str(s)) => {
                values[idx] = s;
                Ok(())
            }
            _ => Err(BasicError::new(ErrorCode::TypeMismatch)),
        }
    }

    fn set_direct_1d(&mut self, index: i32, value: Value) -> BasicResult<()> {
        if self.dims.len() != 1 {
            return Err(BasicError::new(ErrorCode::InvalidIndex));
        }
        if index < 0 || index as usize > self.dims[0] {
            return Err(BasicError::new(ErrorCode::IndexOutOfRange));
        }
        match (&mut self.data, value) {
            (ArrayData::Number(values), Value::Number(n)) => {
                values[index as usize] = n;
                Ok(())
            }
            (ArrayData::Str(values), Value::Str(s)) => {
                values[index as usize] = s;
                Ok(())
            }
            _ => Err(BasicError::new(ErrorCode::TypeMismatch)),
        }
    }

    fn indexes_for_flat(&self, mut flat: usize) -> Vec<i32> {
        let mut indexes = vec![0; self.dims.len()];
        for (slot, bound) in indexes.iter_mut().rev().zip(self.dims.iter().rev()) {
            let span = bound + 1;
            *slot = (flat % span) as i32;
            flat /= span;
        }
        indexes
    }

    fn from_numeric_matrix(
        name: &str,
        base: i32,
        rows: usize,
        cols: usize,
        data: Vec<f64>,
    ) -> Self {
        let upper = |count: usize| {
            if base == 1 {
                count
            } else {
                count.saturating_sub(1)
            }
        };
        let dims = if cols == 1 {
            vec![upper(rows)]
        } else {
            vec![upper(rows), upper(cols)]
        };
        let mut array = ArrayValue::new(name, dims);
        let lower = base.max(0) as usize;
        for r in 0..rows {
            for c in 0..cols {
                let value = Value::number(data[r * cols + c]);
                let indexes = if array.dims.len() == 1 {
                    vec![(lower + r) as i32]
                } else {
                    vec![(lower + r) as i32, (lower + c) as i32]
                };
                let _ = array.set(&indexes, value);
            }
        }
        array
    }
}

#[derive(Debug, Clone)]
enum UserFunction {
    Single {
        params: Vec<String>,
        expr: String,
    },
    Multi {
        params: Vec<String>,
        local_specs: Vec<LocalSpec>,
        start: Cursor,
        end: Cursor,
    },
}

#[derive(Debug, Clone)]
struct ActiveFunctionFrame {
    name: String,
    return_value: Option<Value>,
}

#[derive(Debug, Clone)]
struct UserSub {
    params: Vec<String>,
    local_specs: Vec<LocalSpec>,
    start: Cursor,
    end: Cursor,
}

#[derive(Debug, Clone)]
struct ActiveSubFrame {
    name: String,
}

#[derive(Debug, Clone)]
enum LocalSpec {
    Scalar(String),
    Array { name: String, dims: Vec<String> },
}

#[derive(Debug, Clone)]
struct MatInputEntry {
    name: String,
    positions: Vec<Vec<i32>>,
    position: usize,
}

#[derive(Debug, Clone)]
struct MatStats {
    sum: f64,
    abs_sum: f64,
    fnorm: f64,
    max: f64,
    max_pos: Option<(i32, i32)>,
    min: f64,
    min_pos: Option<(i32, i32)>,
    max_abs: f64,
    max_abs_pos: Option<(i32, i32)>,
    col_norm: f64,
    col_norm_col: Option<i32>,
    row_norm: f64,
    row_norm_row: Option<i32>,
}

impl Default for MatStats {
    fn default() -> Self {
        Self {
            sum: 0.0,
            abs_sum: 0.0,
            fnorm: 0.0,
            max: 0.0,
            max_pos: None,
            min: 0.0,
            min_pos: None,
            max_abs: 0.0,
            max_abs_pos: None,
            col_norm: 0.0,
            col_norm_col: None,
            row_norm: 0.0,
            row_norm_row: None,
        }
    }
}

#[derive(Debug, Clone)]
struct RuntimeErrorState {
    number: i32,
    line: i32,
    retry: Cursor,
    next: Cursor,
}

#[derive(Debug, Clone)]
struct CompiledAssignment {
    targets: Vec<CompiledLValue>,
    rhs: Expr,
    rhs_is_string: bool,
}

#[derive(Debug, Clone)]
struct CompiledMidAssignment {
    target: String,
    start: Expr,
    count: Option<Expr>,
    rhs: Expr,
}

#[derive(Debug, Clone)]
enum CompiledLValue {
    Scalar {
        name: String,
        is_string: bool,
    },
    Array {
        name: String,
        indexes: Vec<Expr>,
        is_string: bool,
    },
}

#[derive(Debug, Clone)]
enum CachedCommand {
    Noop,
    Raw(Rc<str>),
    Assignment(Rc<CompiledAssignment>),
    MidAssignment(Rc<CompiledMidAssignment>),
    StringCharAssignment {
        target: String,
        source: String,
        index: Rc<Expr>,
    },
    DrawRelative2 {
        x: Rc<Expr>,
        y: Rc<Expr>,
    },
    If {
        condition: Rc<Expr>,
        then_branch: CachedIfBranch,
        else_branch: Option<CachedIfBranch>,
    },
    OnGoto {
        selector: Rc<Expr>,
        targets: Vec<i32>,
    },
    OnGosub {
        selector: Rc<Expr>,
        targets: Vec<i32>,
    },
    For(Rc<CompiledFor>),
    GotoConst {
        line: i32,
        target: Option<Cursor>,
    },
    GosubConst {
        line: i32,
        target: Option<Cursor>,
    },
    Return,
    Next(Option<String>),
    While(Rc<Expr>),
    Wend,
}

#[derive(Debug, Clone)]
enum CachedIfBranch {
    Line(i32),
    Commands(Vec<Rc<CachedCommand>>),
}

impl CompiledLValue {
    fn name(&self) -> &str {
        match self {
            CompiledLValue::Scalar { name, .. } | CompiledLValue::Array { name, .. } => name,
        }
    }
}

#[derive(Debug)]
pub struct Interpreter {
    pub program: Program,
    command_cache: HashMap<i32, Vec<Rc<str>>>,
    compiled_command_cache: HashMap<i32, Vec<Rc<CachedCommand>>>,
    line_index_cache: HashMap<i32, usize>,
    next_after_for_cache: HashMap<Cursor, Cursor>,
    wend_after_while_cache: HashMap<Cursor, Cursor>,
    pub root_dir: PathBuf,
    pub current_dir: PathBuf,
    pub program_dir: Option<PathBuf>,
    numeric_variables: HashMap<String, f64>,
    string_variables: HashMap<String, String>,
    identifier_case: HashMap<String, String>,
    arrays: HashMap<String, ArrayValue>,
    data: Vec<Value>,
    data_line_starts: HashMap<i32, usize>,
    data_pointer: usize,
    functions: HashMap<String, UserFunction>,
    subs: HashMap<String, UserSub>,
    function_call_stack: Vec<String>,
    sub_call_stack: Vec<String>,
    active_functions: Vec<ActiveFunctionFrame>,
    active_subs: Vec<ActiveSubFrame>,
    fn_line_owner: HashMap<i32, String>,
    sub_line_owner: HashMap<i32, String>,
    expression_cache: HashMap<String, Rc<Expr>>,
    assignment_cache: HashMap<String, Rc<CompiledAssignment>>,
    for_stack: Vec<ForFrame>,
    while_stack: Vec<WhileFrame>,
    if_stack: Vec<Cursor>,
    gosub_stack: Vec<Cursor>,
    pending_if_branch: Option<Cursor>,
    timers: Vec<BasicTimer>,
    mouse_handlers: HashMap<String, i32>,
    mouse_state: MouseSnapshot,
    mouse_event_consumed: bool,
    interrupts_enabled: bool,
    handling_mouse_event: bool,
    error_handler_line: Option<i32>,
    error_resume_next: bool,
    handling_error: bool,
    last_error: Option<RuntimeErrorState>,
    stopped_cursor: Option<Cursor>,
    key_queue: VecDeque<u8>,
    output: String,
    stream_output: bool,
    line_open: bool,
    output_col: usize,
    ansi_output: bool,
    debug_dirty_blocks: bool,
    debug_block_size: i32,
    pub graphics: Graphics,
    graphics_window: Option<GraphicsWindow>,
    graphics_window_enabled: bool,
    graphics_window_suppressed: bool,
    graphics_window_dirty: bool,
    graphics_window_used_this_run: bool,
    last_graphics_window_pump: Instant,
    last_graphics_window_present: Instant,
    last_frame_command_present: Option<Instant>,
    rng: SimpleRng,
    mat_base: i32,
    angle_degrees: bool,
    trace: bool,
    end_requested: bool,
    function_return_requested: bool,
    sub_return_requested: bool,
    repeat_current_command: bool,
    restart_run_loop: bool,
    program_structure_changed: bool,
    run_depth: usize,
    test_interrupt_requested: bool,
    current_line: Option<i32>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        let root_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            program: Program::default(),
            command_cache: HashMap::new(),
            compiled_command_cache: HashMap::new(),
            line_index_cache: HashMap::new(),
            next_after_for_cache: HashMap::new(),
            wend_after_while_cache: HashMap::new(),
            root_dir: root_dir.clone(),
            current_dir: root_dir,
            program_dir: None,
            numeric_variables: HashMap::new(),
            string_variables: HashMap::new(),
            identifier_case: HashMap::new(),
            arrays: HashMap::new(),
            data: Vec::new(),
            data_line_starts: HashMap::new(),
            data_pointer: 0,
            functions: HashMap::new(),
            subs: HashMap::new(),
            function_call_stack: Vec::new(),
            sub_call_stack: Vec::new(),
            active_functions: Vec::new(),
            active_subs: Vec::new(),
            fn_line_owner: HashMap::new(),
            sub_line_owner: HashMap::new(),
            expression_cache: HashMap::new(),
            assignment_cache: HashMap::new(),
            for_stack: Vec::new(),
            while_stack: Vec::new(),
            if_stack: Vec::new(),
            gosub_stack: Vec::new(),
            pending_if_branch: None,
            timers: Vec::new(),
            mouse_handlers: HashMap::new(),
            mouse_state: MouseSnapshot::default(),
            mouse_event_consumed: false,
            interrupts_enabled: true,
            handling_mouse_event: false,
            error_handler_line: None,
            error_resume_next: false,
            handling_error: false,
            last_error: None,
            stopped_cursor: None,
            key_queue: VecDeque::new(),
            output: String::new(),
            stream_output: false,
            line_open: false,
            output_col: 0,
            ansi_output: console::ansi_enabled(),
            debug_dirty_blocks: false,
            debug_block_size: 32,
            graphics: Graphics::default(),
            graphics_window: None,
            graphics_window_enabled: graphics_window_enabled(),
            graphics_window_suppressed: false,
            graphics_window_dirty: false,
            graphics_window_used_this_run: false,
            last_graphics_window_pump: Instant::now(),
            last_graphics_window_present: Instant::now(),
            last_frame_command_present: None,
            rng: SimpleRng::new(5489),
            mat_base: 0,
            angle_degrees: false,
            trace: false,
            end_requested: false,
            function_return_requested: false,
            sub_return_requested: false,
            repeat_current_command: false,
            restart_run_loop: false,
            program_structure_changed: false,
            run_depth: 0,
            test_interrupt_requested: false,
            current_line: None,
        }
    }

    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.output)
    }

    pub fn request_interrupt_for_test(&mut self) {
        self.test_interrupt_requested = true;
    }

    pub fn set_stream_output(&mut self, enabled: bool) {
        self.stream_output = enabled;
    }

    pub fn print_banner(&mut self) {
        self.write_line("AVL BASIC v1.5");
        self.write_line("BASIC interpreter written in Rust");
        self.write_line("Copyright 2024-2026 Jos\u{00e9} Antonio \u{00c1}vila");
        self.write_line("License: GPLv3 or later (see COPYING)");
        self.write_line(
            "This is free software under GPLv3 or later. You may redistribute it under its terms.",
        );
        self.write_line("This program comes with ABSOLUTELY NO WARRANTY. See COPYING.");
    }

    pub fn repl(&mut self) -> i32 {
        let _ = console::install_ctrl_c_handler();
        self.set_stream_output(true);
        self.print_banner();
        print!("{}", self.take_output());
        let mut suppress_ready = false;
        loop {
            if !suppress_ready {
                println!("{}", console::prompt_text(self.ansi_output, "Ready"));
            }
            let identifier_case = self.identifier_case.clone();
            match console::read_highlighted_line_with_idle(
                "",
                "",
                self.ansi_output,
                Some(&identifier_case),
                || self.pump_graphics_window_for_console(),
            ) {
                Ok(line) => {
                    let line = line.trim_end_matches(&['\r', '\n'][..]);
                    let normalized = console::normalize_code(line);
                    let is_program_line = starts_with_line_number(normalized.trim());
                    if line.eq_ignore_ascii_case("EXIT")
                        || line.eq_ignore_ascii_case("QUIT")
                        || line.eq_ignore_ascii_case("SYSTEM")
                    {
                        return 0;
                    }
                    if let Err(err) = self.process_immediate(line) {
                        suppress_ready = false;
                        self.finish_output_line();
                        let text = err.display_for_basic();
                        self.write_line(&console::error_text(self.ansi_output, &text));
                    } else {
                        suppress_ready = is_program_line || line.trim().is_empty();
                    }
                    print!("{}", self.take_output());
                }
                Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return 0,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    println!(
                        "{}",
                        console::error_text(
                            self.ansi_output,
                            "Interrupted by user. Exiting interpreter."
                        )
                    );
                    return 0;
                }
                Err(_) => return 1,
            }
        }
    }

    pub fn load_file(&mut self, path: &Path) -> BasicResult<()> {
        let text = read_text_file(path)?;
        self.program.load_text(&text)?;
        self.clear_runtime();
        self.clear_command_caches();
        self.program_dir = path.parent().map(Path::to_path_buf);
        self.refresh_identifier_case_from_program();
        Ok(())
    }

    fn refresh_identifier_case_from_program(&mut self) {
        self.identifier_case.clear();
        let codes: Vec<String> = self
            .program
            .line_numbers()
            .into_iter()
            .filter_map(|line| self.program.get(line).map(str::to_string))
            .collect();
        for code in codes {
            record_identifier_case_forms(&code, &mut self.identifier_case, false);
        }
    }

    fn record_identifier_case_from_numbered_line(&mut self, source: &str, overwrite: bool) {
        if let Some(code) = numbered_line_code(source) {
            self.record_identifier_case_from_code(code, overwrite);
        }
    }

    fn record_identifier_case_from_code(&mut self, source: &str, overwrite: bool) {
        record_identifier_case_forms(source, &mut self.identifier_case, overwrite);
    }

    pub fn run_loaded(&mut self) -> BasicResult<RunOutcome> {
        self.run_loaded_from(None)
    }

    pub fn run_loaded_from(&mut self, start_line: Option<i32>) -> BasicResult<RunOutcome> {
        self.prepare_run();
        self.rebuild_data();
        self.rebuild_command_cache();
        let line_idx = if let Some(line) = start_line {
            self.line_index(line)
                .ok_or_else(|| self.err(ErrorCode::TargetLineNotFound))?
        } else {
            0
        };
        self.run_from(Cursor {
            line_idx,
            cmd_idx: 0,
        })
    }

    pub fn process_immediate(&mut self, line: &str) -> BasicResult<()> {
        let normalized = console::normalize_code(line);
        let trimmed = normalized.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.current_line = None;
        if starts_with_line_number(trimmed) {
            self.program.add_source_line(trimmed)?;
            if numbered_line_code(trimmed).is_some_and(|code| code.trim().is_empty()) {
                self.refresh_identifier_case_from_program();
            } else {
                self.record_identifier_case_from_numbered_line(trimmed, false);
            }
            self.clear_command_caches();
            return Ok(());
        }
        let upper = trimmed.to_ascii_uppercase();
        if upper == "NEW" {
            self.program.clear();
            self.clear_command_caches();
            self.clear_runtime();
            self.identifier_case.clear();
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "LIST") {
            self.output.push_str(&self.render_program_list_range(arg)?);
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "FILES") {
            self.execute_files(arg)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "CAT") {
            self.execute_files(arg)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "CD") {
            self.execute_cd(arg)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "SAVE") {
            let path = self.resolve_bas_literal_arg(arg)?;
            self.save_file(&path)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "LOAD") {
            let path = self.resolve_bas_literal_arg(arg)?;
            self.load_file(&path)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "RUN") {
            let arg = arg.trim();
            let start_line = if arg.is_empty() {
                None
            } else if arg.starts_with('"') {
                let path = self.resolve_bas_literal_arg(arg)?;
                self.load_file(&path)?;
                None
            } else if let Some(line) = parse_line_number_literal(arg) {
                Some(line)
            } else {
                let _ = self.resolve_bas_literal_arg(arg)?;
                None
            };
            let result = self.run_loaded_from(start_line);
            self.current_line = None;
            self.program_dir = None;
            result?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "CONT") {
            if !arg.trim().is_empty() {
                return Err(self.err(ErrorCode::Syntax));
            }
            let cursor = self
                .stopped_cursor
                .clone()
                .ok_or_else(|| BasicError::new(ErrorCode::NoStoppedProgram))?;
            self.stopped_cursor = None;
            self.run_from(cursor)?;
            return Ok(());
        }
        let commands = split_commands(trimmed);
        if let Some(first_command) = commands.first() {
            let first_upper = first_command.to_ascii_uppercase();
            if let Some(arg) = immediate_arg(first_command, &first_upper, "GOTO") {
                self.execute_immediate_goto(arg)?;
                return Ok(());
            }
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "RENUM") {
            self.execute_renum(arg)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "DELETE") {
            self.execute_delete_lines(arg)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "EDIT") {
            self.execute_edit_line(arg)?;
            return Ok(());
        }
        if let Some(arg) = immediate_arg(trimmed, &upper, "DEBUG") {
            self.execute_debug(arg)?;
            return Ok(());
        }
        self.record_identifier_case_from_code(trimmed, false);
        let mut cursor = Cursor {
            line_idx: 0,
            cmd_idx: 0,
        };
        while cursor.line_idx == 0 && cursor.cmd_idx < commands.len() {
            let command = commands[cursor.cmd_idx].clone();
            let before = cursor.clone();
            let stopped_before = self.stopped_cursor.clone();
            let next = Cursor {
                line_idx: cursor.line_idx,
                cmd_idx: cursor.cmd_idx + 1,
            };
            self.execute_command(&command, &mut cursor, &commands)?;
            if self.end_requested
                || (self.stopped_cursor.is_some() && self.stopped_cursor != stopped_before)
                || self.function_return_requested
                || self.sub_return_requested
            {
                break;
            }
            if self.repeat_current_command {
                self.repeat_current_command = false;
                continue;
            }
            if cursor != before {
                continue;
            }
            cursor = next;
        }
        self.end_requested = false;
        self.function_return_requested = false;
        self.sub_return_requested = false;
        Ok(())
    }

    fn execute_immediate_goto(&mut self, arg: &str) -> BasicResult<()> {
        let line = parse_line_number_literal(arg.trim())
            .ok_or_else(|| self.err(ErrorCode::InvalidLineNumber))?;
        if self.command_cache.is_empty() || self.compiled_command_cache.is_empty() {
            self.rebuild_command_cache();
        }
        let line_idx = self
            .line_index(line)
            .ok_or_else(|| self.err(ErrorCode::TargetLineNotFound))?;
        if self.stopped_cursor.is_none() {
            return Err(self.err(ErrorCode::NoStoppedProgram));
        }
        self.stopped_cursor = None;
        self.end_requested = false;
        self.function_return_requested = false;
        self.sub_return_requested = false;
        self.repeat_current_command = false;
        self.restart_run_loop = false;
        let result = self.run_from(Cursor {
            line_idx,
            cmd_idx: 0,
        });
        self.current_line = None;
        result?;
        Ok(())
    }

    fn execute_renum(&mut self, args: &str) -> BasicResult<()> {
        let parts = if args.trim().is_empty() {
            Vec::new()
        } else {
            args.split(',')
                .map(|part| part.trim().to_string())
                .collect()
        };
        if parts.len() > 3 {
            return Err(self.err(ErrorCode::Syntax));
        }
        let first_line = self.program.line_numbers().into_iter().next().unwrap_or(10);
        let new_start = if let Some(part) = parts.get(0).filter(|part| !part.trim().is_empty()) {
            parse_line_number_literal(part).ok_or_else(|| self.err(ErrorCode::Syntax))?
        } else {
            10
        };
        let step = if let Some(part) = parts.get(1).filter(|part| !part.trim().is_empty()) {
            parse_line_number_literal(part).ok_or_else(|| self.err(ErrorCode::Syntax))?
        } else {
            10
        };
        let old_start = if let Some(part) = parts.get(2).filter(|part| !part.trim().is_empty()) {
            parse_line_number_literal(part).ok_or_else(|| self.err(ErrorCode::Syntax))?
        } else {
            first_line
        };
        if parts.iter().any(|part| part.trim().is_empty()) && !parts.is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }
        if new_start <= 0 || step <= 0 || old_start <= 0 {
            return Err(self.err(ErrorCode::InvalidArgument));
        }

        let old_lines = self.program.line_numbers();
        let renumbered: Vec<i32> = old_lines
            .iter()
            .copied()
            .filter(|line| *line >= old_start)
            .collect();
        if renumbered.is_empty() {
            return Ok(());
        }
        if let Some(max_before) = old_lines
            .iter()
            .copied()
            .filter(|line| *line < old_start)
            .max()
        {
            if new_start <= max_before {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
        }

        let mut mapping = HashMap::new();
        let mut next = new_start;
        for old in &renumbered {
            mapping.insert(*old, next);
            next = next
                .checked_add(step)
                .ok_or_else(|| self.err(ErrorCode::InvalidArgument))?;
        }

        let mut used = std::collections::HashSet::new();
        for old in &old_lines {
            let new_no = mapping.get(old).copied().unwrap_or(*old);
            if new_no <= 0 || !used.insert(new_no) {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
        }

        let mut text = String::new();
        for old in old_lines {
            let new_no = mapping.get(&old).copied().unwrap_or(old);
            let code = self.program.get(old).unwrap_or("");
            let code = renumber_line_references(code, &mapping);
            text.push_str(&format!("{new_no}{code}\n"));
        }
        self.program.load_text(&text)?;
        self.clear_command_caches();
        self.refresh_identifier_case_from_program();
        self.rebuild_command_cache();
        Ok(())
    }

    fn execute_cd(&mut self, args: &str) -> BasicResult<()> {
        if args.trim().is_empty() {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let path = extract_quoted_path_text(args)?;
        let target = self.resolve_virtual_directory(&path)?;
        if !target.exists() || !target.is_dir() {
            return Err(self.err(ErrorCode::FileNotFound));
        }
        self.current_dir = target;
        Ok(())
    }

    fn execute_files(&mut self, args: &str) -> BasicResult<()> {
        let pattern = if args.trim().is_empty() {
            "*.bas".to_string()
        } else {
            resolve_wildcard_pattern(args)?
        };
        let mut entries = Vec::new();
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.current_dir)
            .map_err(|e| self.err(ErrorCode::InvalidValue).with_detail(e.to_string()))?
        {
            let entry =
                entry.map_err(|e| self.err(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let kind = entry
                .file_type()
                .map_err(|e| self.err(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
            if kind.is_dir() || entry.path().is_dir() {
                dirs.push(format!("{name}/"));
            } else if wildcard_matches(&pattern, &name) {
                files.push(name);
            }
        }
        dirs.sort_by_key(|name| name.to_ascii_lowercase());
        files.sort_by_key(|name| name.to_ascii_lowercase());
        entries.extend(dirs);
        entries.extend(files);

        for chunk in entries.chunks(4) {
            let mut line = String::new();
            for entry in chunk {
                line.push_str(&format!("{entry:<20}"));
            }
            self.write_line(&line);
        }
        Ok(())
    }

    fn execute_delete_lines(&mut self, args: &str) -> BasicResult<()> {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            self.program.clear();
            self.clear_command_caches();
            self.identifier_case.clear();
            return Ok(());
        }
        let range = parse_line_range_spec(trimmed, ErrorCode::Syntax, ErrorCode::InvalidArgument)?;
        let lines = self.program.line_numbers();
        if let LineRangeSpec::Single(line) = range {
            if !lines.contains(&line) {
                return Err(self.err(ErrorCode::TargetLineNotFound));
            }
            self.program.delete_range(line, line);
        } else {
            let (start, end) = range.bounds_for(&lines);
            self.program.delete_range(start, end);
        }
        self.clear_command_caches();
        self.refresh_identifier_case_from_program();
        Ok(())
    }

    fn execute_edit_line(&mut self, args: &str) -> BasicResult<()> {
        let line = parse_line_number_literal(args.trim())
            .ok_or_else(|| self.err(ErrorCode::InvalidArgument))?;
        let Some(code) = self.program.get(line) else {
            return Err(self.err(ErrorCode::TargetLineNotFound));
        };
        if !console::interactive_terminal() {
            return Err(self.err(ErrorCode::Unsupported));
        }
        let prefill = format!("{line}{code}");
        let edited = console::read_highlighted_line("", &prefill, self.ansi_output, None)
            .map_err(|e| self.err(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
        let normalized = console::normalize_code(&edited);
        if !normalized.trim().is_empty() {
            self.program.add_source_line(&normalized)?;
            if numbered_line_code(&normalized).is_some_and(|code| code.trim().is_empty()) {
                self.refresh_identifier_case_from_program();
            } else {
                self.record_identifier_case_from_numbered_line(&normalized, true);
            }
            self.clear_command_caches();
        }
        Ok(())
    }

    fn execute_debug(&mut self, args: &str) -> BasicResult<()> {
        let arg = args.trim();
        if arg.is_empty() {
            self.debug_dirty_blocks = !self.debug_dirty_blocks;
        } else {
            self.debug_block_size = self.eval_number(arg)? as i32;
            self.debug_dirty_blocks = true;
        }
        if self.debug_dirty_blocks {
            self.write_line(&format!(
                "Graphics debugging: ON (block_size = {})",
                self.debug_block_size
            ));
        } else {
            self.write_line("Graphics debugging: OFF");
        }
        Ok(())
    }

    fn save_file(&mut self, path: &Path) -> BasicResult<()> {
        let mut text = String::new();
        for line in self.program.line_numbers() {
            let code = self.program.get(line).unwrap_or("");
            let code = apply_identifier_case(code, &self.identifier_case);
            text.push_str(&format!("{line}{code}\n"));
        }
        fs::write(path, text)
            .map_err(|e| self.err(ErrorCode::InvalidValue).with_detail(e.to_string()))
    }

    fn resolve_virtual_directory(&self, path: &str) -> BasicResult<PathBuf> {
        self.resolve_virtual_path_text(path, &self.current_dir)
    }

    fn resolve_virtual_path_text(&self, path: &str, base_dir: &Path) -> BasicResult<PathBuf> {
        let normalized = path.replace('\\', "/");
        if normalized.starts_with("//") || has_windows_drive_prefix(&normalized) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }

        let root = self.virtual_root_for_base(base_dir);
        let mut parts = if normalized.starts_with('/') {
            Vec::new()
        } else {
            let base = if base_dir.is_absolute() {
                base_dir.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(base_dir)
            };
            let base = if base.starts_with(&root) {
                base
            } else {
                base.canonicalize().unwrap_or(base)
            };
            base.strip_prefix(&root)
                .map_err(|_| self.err(ErrorCode::InvalidArgument))?
                .components()
                .map(|component| component.as_os_str().to_os_string())
                .collect::<Vec<_>>()
        };

        for part in normalized.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                parts.pop();
            } else {
                parts.push(part.into());
            }
        }

        let mut candidate = root.clone();
        for part in parts {
            candidate.push(part);
        }
        let resolved = if candidate.exists() {
            let canonical = candidate
                .canonicalize()
                .map_err(|e| self.err(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
            if canonical.starts_with(&root) {
                canonical
            } else {
                candidate
            }
        } else {
            candidate
        };
        if !resolved.starts_with(&root) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        Ok(resolved)
    }

    fn canonical_root_dir(&self) -> PathBuf {
        self.root_dir
            .canonicalize()
            .unwrap_or_else(|_| self.root_dir.clone())
    }

    fn virtual_root_for_base(&self, base_dir: &Path) -> PathBuf {
        let base = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());
        if let Some(oracle_root) = self.oracle_root_dir() {
            if base.starts_with(&oracle_root) {
                return oracle_root;
            }
        }
        self.canonical_root_dir()
    }

    fn oracle_root_dir(&self) -> Option<PathBuf> {
        let parent = self.root_dir.parent()?;
        let candidates = [
            // Monorepo layout: AVL-BASIC/rust is inside the Python reference repo.
            parent.to_path_buf(),
            // Legacy development layout: AVL-BASIC and BASIC-rust are siblings.
            parent.join("AVL-BASIC"),
        ];
        candidates
            .into_iter()
            .find(|path| path.join("basic.py").is_file() && path.join("samples").is_dir())
            .and_then(|path| path.canonicalize().ok())
    }

    fn prepare_run(&mut self) {
        self.numeric_variables.clear();
        self.string_variables.clear();
        self.arrays.clear();
        self.data_pointer = 0;
        self.for_stack.clear();
        self.while_stack.clear();
        self.if_stack.clear();
        self.gosub_stack.clear();
        self.pending_if_branch = None;
        self.timers.clear();
        self.mouse_handlers.clear();
        self.mouse_state = MouseSnapshot::default();
        self.mouse_event_consumed = false;
        self.interrupts_enabled = true;
        self.handling_mouse_event = false;
        self.graphics_window_suppressed = false;
        self.last_frame_command_present = None;
        self.graphics_window_used_this_run = false;
        self.error_handler_line = None;
        self.error_resume_next = false;
        self.handling_error = false;
        self.last_error = None;
        self.stopped_cursor = None;
        self.end_requested = false;
        self.function_return_requested = false;
        self.sub_return_requested = false;
        self.repeat_current_command = false;
        self.restart_run_loop = false;
        self.program_structure_changed = false;
        self.current_line = None;
        self.mat_base = 0;
        self.angle_degrees = false;
    }

    fn clear_runtime(&mut self) {
        self.prepare_run();
        self.functions.clear();
        self.subs.clear();
        self.function_call_stack.clear();
        self.sub_call_stack.clear();
        self.active_functions.clear();
        self.active_subs.clear();
        self.fn_line_owner.clear();
        self.sub_line_owner.clear();
        self.error_handler_line = None;
        self.error_resume_next = false;
        self.expression_cache.clear();
        self.graphics.reset_state();
    }

    fn run_from(&mut self, cursor: Cursor) -> BasicResult<RunOutcome> {
        self.run_depth += 1;
        let _runtime_raw = console::enter_runtime_raw_mode().ok();
        let result = self.run_from_inner(cursor);
        self.run_depth -= 1;
        result
    }

    fn should_present_at_run_boundary(&self) -> bool {
        self.run_depth <= 1
    }

    fn run_from_inner(&mut self, mut cursor: Cursor) -> BasicResult<RunOutcome> {
        let mut lines = self.program.line_numbers();
        'run_loop: loop {
            if cursor.line_idx >= lines.len() {
                break;
            }
            let line_no = lines[cursor.line_idx];
            let commands_len = self
                .compiled_command_cache
                .get(&line_no)
                .map_or(0usize, |commands| commands.len());
            if cursor.cmd_idx >= commands_len {
                cursor.line_idx += 1;
                cursor.cmd_idx = 0;
                continue;
            }
            self.current_line = Some(line_no);
            self.check_user_interrupt(&cursor)?;
            self.process_timers()?;
            self.check_user_interrupt(&cursor)?;
            if self.end_requested {
                if self.should_present_at_run_boundary() && self.graphics_window_dirty {
                    self.present_graphics_window()?;
                }
                return Ok(RunOutcome::End);
            }
            if self.stopped_cursor.is_some() {
                if self.should_present_at_run_boundary() && self.graphics_window_dirty {
                    self.present_graphics_window()?;
                }
                return Ok(RunOutcome::Stop);
            }
            if self.trace {
                self.write(&console::trace_text(self.ansi_output, line_no));
            }
            while cursor.cmd_idx < commands_len {
                self.check_user_interrupt(&cursor)?;
                let command = self
                    .compiled_command_cache
                    .get(&line_no)
                    .and_then(|commands| commands.get(cursor.cmd_idx))
                    .cloned()
                    .unwrap_or_else(|| Rc::new(CachedCommand::Noop));
                let before = cursor.clone();
                let next = Cursor {
                    line_idx: cursor.line_idx,
                    cmd_idx: cursor.cmd_idx + 1,
                };
                if let Err(err) = self.execute_cached_command(command.as_ref(), &mut cursor, &[]) {
                    let err = if err
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.starts_with("Error in error handler:"))
                    {
                        err
                    } else {
                        self.with_current_line(err)
                    };
                    if err.code == ErrorCode::KeyboardInterrupt {
                        return Err(err);
                    }
                    if self.handle_runtime_error(
                        err.clone(),
                        &mut cursor,
                        before.clone(),
                        next.clone(),
                    )? {
                        if cursor.line_idx != before.line_idx {
                            continue 'run_loop;
                        }
                        continue;
                    }
                    return Err(err);
                }
                if self.end_requested {
                    if self.should_present_at_run_boundary() && self.graphics_window_dirty {
                        self.present_graphics_window()?;
                    }
                    return Ok(RunOutcome::End);
                }
                if self.stopped_cursor.is_some() {
                    if self.should_present_at_run_boundary() && self.graphics_window_dirty {
                        self.present_graphics_window()?;
                    }
                    return Ok(RunOutcome::Stop);
                }
                if self.function_return_requested {
                    return Ok(RunOutcome::End);
                }
                if self.sub_return_requested {
                    return Ok(RunOutcome::End);
                }
                if self.restart_run_loop {
                    self.restart_run_loop = false;
                    lines = self.program.line_numbers();
                    continue 'run_loop;
                }
                if self.program_structure_changed {
                    self.program_structure_changed = false;
                    lines = self.program.line_numbers();
                }
                if self.repeat_current_command {
                    self.repeat_current_command = false;
                    continue;
                }
                if cursor != before {
                    if cursor.line_idx != before.line_idx {
                        continue 'run_loop;
                    }
                    continue;
                }
                cursor = next;
            }
            cursor.line_idx += 1;
            cursor.cmd_idx = 0;
        }
        if let Some(frame) = self.if_stack.last() {
            let line = lines
                .get(frame.line_idx)
                .copied()
                .or_else(|| self.program.line_numbers().get(frame.line_idx).copied())
                .unwrap_or_default();
            return Err(BasicError::new(ErrorCode::IfWithoutEndIf).at_line(line));
        }
        if self.should_present_at_run_boundary() && self.graphics_window_dirty {
            self.present_graphics_window()?;
        }
        if cursor.line_idx != usize::MAX {
            self.finish_output_line();
        }
        Ok(RunOutcome::End)
    }

    fn check_user_interrupt(&mut self, cursor: &Cursor) -> BasicResult<()> {
        self.pump_graphics_window_if_due()?;
        if !self.test_interrupt_requested && !console::take_interrupt_requested() {
            return Ok(());
        }
        self.test_interrupt_requested = false;
        self.finish_output_line();
        self.stopped_cursor = Some(cursor.clone());
        self.end_requested = false;
        self.function_return_requested = false;
        self.sub_return_requested = false;
        Err(BasicError::new(ErrorCode::KeyboardInterrupt))
    }

    fn execute_command(
        &mut self,
        command: &str,
        cursor: &mut Cursor,
        line_commands: &[String],
    ) -> BasicResult<()> {
        let command = command.trim();
        if command.is_empty() {
            return Ok(());
        }
        let upper = command.to_ascii_uppercase();
        let first = upper.split_whitespace().next().unwrap_or("");
        if self.current_line.is_none() && is_non_immediate_command(first) {
            return Err(self.err(ErrorCode::NonImmediateCommand));
        }
        if self.current_line.is_some() && is_immediate_only_command(first, &upper) {
            return Err(self.err(ErrorCode::ImmediateCommand));
        }
        if upper.starts_with("PRINT") {
            return self.execute_print(command[5..].trim());
        }
        if let Some(rest) = command.strip_prefix('?') {
            return self.execute_print(rest.trim());
        }
        if upper.starts_with("LINE INPUT") {
            return self.execute_line_input(command[10..].trim(), cursor);
        }
        if self.inside_multiline_routine()
            && (upper.starts_with("ON ERROR")
                || upper.starts_with("CHAIN MERGE")
                || first == "RESUME"
                || first == "DEF"
                || matches!(first, "AFTER" | "EVERY" | "DI" | "EI" | "CHAIN" | "MERGE"))
        {
            return Err(self.err(if self.inside_multiline_function() {
                ErrorCode::FunctionForbidden
            } else {
                ErrorCode::SubroutineForbidden
            }));
        }
        match first {
            "REM" | "DATA" => Ok(()),
            "PRINT" | "?" => self.execute_print(command[first.len()..].trim()),
            "INPUT" => self.execute_input(command[5..].trim(), cursor),
            "LINE" if upper.starts_with("LINE INPUT") => {
                self.execute_line_input(command[10..].trim(), cursor)
            }
            "LET" => self.execute_assignment(command[3..].trim()),
            "IF" => self.execute_if(command, cursor, line_commands),
            "ELSEIF" => self.execute_elseif(command, cursor, line_commands),
            "ELSE" => self.execute_else(cursor),
            "ON" if upper.starts_with("ON ERROR") => self.execute_on_error(command),
            "ON" if upper.starts_with("ON MOUSE") => self.execute_on_mouse(command),
            "ON" => self.execute_on(command, cursor),
            "ERROR" => self.execute_error(command[5..].trim()),
            "RESUME" => self.execute_resume(command[6..].trim(), cursor),
            "GOTO" => self.jump_to(command[4..].trim(), cursor),
            "GOSUB" => {
                self.gosub_stack.push(Cursor {
                    line_idx: cursor.line_idx,
                    cmd_idx: cursor.cmd_idx + 1,
                });
                self.jump_to_gosub(command[5..].trim(), cursor)
            }
            "RETURN" => {
                let ret = self
                    .gosub_stack
                    .pop()
                    .ok_or_else(|| self.err(ErrorCode::ReturnWithoutGosub))?;
                if !self.if_stack.is_empty() {
                    self.reconcile_if_stack_for_jump(&ret)?;
                }
                *cursor = ret;
                Ok(())
            }
            "FOR" => self.execute_for(command[3..].trim(), cursor),
            "NEXT" => self.execute_next(command[4..].trim(), cursor),
            "WHILE" => self.execute_while(command[5..].trim(), cursor),
            "WEND" => self.execute_wend(cursor),
            "DIM" => self.execute_dim(command[3..].trim()),
            "REDIM" => self.execute_redim(command[5..].trim()),
            "MAT" => self.execute_mat(command[3..].trim(), cursor),
            "LOCAL" => self.execute_local(),
            "MERGE" => self.execute_merge(command[5..].trim()),
            "CHAIN" if upper.starts_with("CHAIN MERGE") => {
                self.execute_chain_merge(command[11..].trim(), cursor)
            }
            "CHAIN" => self.execute_chain(command[5..].trim(), cursor),
            "READ" => self.execute_read(command[4..].trim()),
            "RESTORE" => {
                let arg = command[7..].trim();
                if arg.is_empty() {
                    self.data_pointer = 0;
                } else {
                    let line = self.eval_number(arg)? as i32;
                    self.data_pointer = *self
                        .data_line_starts
                        .get(&line)
                        .ok_or_else(|| self.err(ErrorCode::TargetLineNotFound))?;
                }
                Ok(())
            }
            "AFTER" => self.execute_timer(command[5..].trim(), false),
            "EVERY" => self.execute_timer(command[5..].trim(), true),
            "DI" => {
                self.interrupts_enabled = false;
                Ok(())
            }
            "EI" => {
                self.interrupts_enabled = true;
                self.process_timers()
            }
            "RANDOMIZE" => self.execute_randomize(command[9..].trim()),
            "SWAP" => self.execute_swap(command[4..].trim()),
            "CLEAR" => {
                self.numeric_variables.clear();
                self.string_variables.clear();
                self.arrays.clear();
                self.for_stack.clear();
                self.while_stack.clear();
                self.gosub_stack.clear();
                self.data_pointer = 0;
                Ok(())
            }
            "RAD" => {
                self.angle_degrees = false;
                Ok(())
            }
            "DEG" => {
                self.angle_degrees = true;
                Ok(())
            }
            "DEF" => self.execute_def(command, cursor),
            "CALL" => self.execute_call(command[4..].trim()),
            "FNEND" => self.execute_fnend(),
            "FNEXIT" => self.execute_fnexit(),
            "SUBEND" => self.execute_subend(),
            "SUBEXIT" => self.execute_subexit(),
            "EXIT" if upper == "EXIT FN" => self.execute_fnexit(),
            "EXIT" if upper == "EXIT SUB" => self.execute_subexit(),
            "EXIT" if upper == "EXIT FOR" => self.execute_exit_for(cursor),
            "EXIT" if upper == "EXIT WHILE" => self.execute_exit_while(cursor),
            "END" if upper == "END IF" => self.execute_end_if(cursor),
            "ENDIF" => self.execute_end_if(cursor),
            "END" => {
                self.finish_output_line();
                if self.current_line.is_none() {
                    self.stopped_cursor = None;
                }
                self.end_requested = true;
                Ok(())
            }
            "STOP" => {
                self.finish_output_line();
                self.stopped_cursor = Some(Cursor {
                    line_idx: cursor.line_idx,
                    cmd_idx: cursor.cmd_idx + 1,
                });
                Ok(())
            }
            "TRON" => {
                self.trace = true;
                Ok(())
            }
            "TROFF" => {
                self.trace = false;
                Ok(())
            }
            "PAUSE" => self.execute_pause(command[5..].trim(), cursor),
            "BEEP" => {
                self.write("\x07");
                Ok(())
            }
            "CLS" => {
                self.write("\x1b[2J\x1b[H");
                self.output_col = 0;
                self.line_open = false;
                Ok(())
            }
            "SCREEN" => self.execute_screen(command[6..].trim()),
            "MODE" => {
                self.graphics_window_suppressed = false;
                let width = self.eval_number(command[4..].trim())? as usize;
                self.graphics.set_mode(width)?;
                self.present_graphics_window()
            }
            "CLG" => {
                self.ensure_graphics_window()?;
                self.graphics.clg();
                self.refresh_graphics_window()
            }
            "INK" => {
                self.ensure_graphics_window()?;
                let color = self.eval_color_arguments(command[3..].trim())?;
                self.graphics.set_ink(color);
                Ok(())
            }
            "PAPER" => {
                self.ensure_graphics_window()?;
                let color = self.eval_color_arguments(command[5..].trim())?;
                self.graphics.set_paper(color);
                Ok(())
            }
            "PLOT" => self.execute_plot(command[4..].trim(), false),
            "PLOTR" => self.execute_plot(command[5..].trim(), true),
            "MOVE" => self.execute_move(command[4..].trim(), false),
            "MOVER" => self.execute_move(command[5..].trim(), true),
            "MOUSE" => self.execute_mouse(command[5..].trim()),
            "DRAW" => self.execute_draw(command[4..].trim(), false),
            "DRAWR" => self.execute_draw(command[5..].trim(), true),
            "RECTANGLE" => self.execute_rect(command[9..].trim(), false),
            "FRECTANGLE" => self.execute_rect(command[10..].trim(), true),
            "TRIANGLE" => self.execute_triangle(command[8..].trim(), false),
            "FTRIANGLE" => self.execute_triangle(command[9..].trim(), true),
            "CIRCLE" => self.execute_circle(command[6..].trim(), false),
            "CIRCLER" => self.execute_circle_relative(command[7..].trim(), false),
            "FCIRCLE" => self.execute_circle(command[7..].trim(), true),
            "FCIRCLER" => self.execute_circle_relative(command[8..].trim(), true),
            "FILL" => self.execute_fill(command[4..].trim()),
            "ORIGIN" => {
                self.ensure_graphics_window()?;
                let args = self.eval_numbers(command[6..].trim())?;
                if !matches!(args.len(), 2 | 6) {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                let viewport = if args.len() == 6 {
                    Some((
                        args[2] as i32,
                        args[3] as i32,
                        args[4] as i32,
                        args[5] as i32,
                    ))
                } else {
                    None
                };
                self.graphics
                    .set_origin(args[0] as i32, args[1] as i32, viewport)
            }
            "SCALE" => self.execute_scale(command[5..].trim()),
            "GRAPHRANGE" => self.execute_graph_range(command[10..].trim()),
            "CROSSAT" => self.execute_cross_at(command[7..].trim()),
            "XAXIS" => self.execute_xaxis(command[5..].trim()),
            "YAXIS" => self.execute_yaxis(command[5..].trim()),
            "GRAPH" => self.execute_graph(command[5..].trim()),
            "LOCATE" => {
                self.ensure_graphics_window()?;
                let args = self.eval_numbers(command[6..].trim())?;
                if args.len() != 2 {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                self.graphics.locate(args[0] as i32, args[1] as i32);
                Ok(())
            }
            "DISP" => self.execute_disp(command[4..].trim(), false),
            "GDISP" => self.execute_disp(command[5..].trim(), true),
            "FRAME" => self.execute_frame(command[5..].trim(), cursor),
            "MASK" => {
                self.ensure_graphics_window()?;
                let arg = command[4..].trim();
                let mask = if arg.is_empty() {
                    None
                } else {
                    Some(self.eval_number(arg)? as i32)
                };
                self.graphics.set_mask(mask)
            }
            "PENWIDTH" => {
                self.ensure_graphics_window()?;
                let width = self.eval_number(command[8..].trim())? as i32;
                self.graphics.set_pen_width(width)
            }
            "SMALLFONT" => {
                self.ensure_graphics_window()?;
                if !command[9..].trim().is_empty() {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                self.graphics.set_font(FontKind::Small);
                Ok(())
            }
            "BIGFONT" => {
                self.ensure_graphics_window()?;
                if !command[7..].trim().is_empty() {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                self.graphics.set_font(FontKind::Big);
                Ok(())
            }
            "LDIR" => {
                self.ensure_graphics_window()?;
                let angle = self.eval_number(command[4..].trim())? as i32;
                self.graphics.set_ldir(angle);
                Ok(())
            }
            "SPRITE" => self.execute_sprite(command[6..].trim()),
            "COLMODE" => {
                self.ensure_graphics_window()?;
                let mode = self.eval_number(command[7..].trim())? as i32;
                self.graphics.colmode(mode)
            }
            "COLCOLOR" => {
                self.ensure_graphics_window()?;
                let arg = command[8..].trim();
                let color = if arg.is_empty() {
                    None
                } else {
                    Some(self.eval_color(arg)?)
                };
                self.graphics.colcolor(color);
                Ok(())
            }
            "COLRESET" => {
                self.ensure_graphics_window()?;
                self.graphics.colreset();
                Ok(())
            }
            "BSAVE" => self.execute_bsave(command[5..].trim()),
            "BLOAD" => self.execute_bload(command[5..].trim()),
            _ if is_assignment(command) => self.execute_assignment(command),
            _ => Err(self.err(ErrorCode::Syntax)),
        }
    }

    fn execute_cached_command(
        &mut self,
        command: &CachedCommand,
        cursor: &mut Cursor,
        line_commands: &[String],
    ) -> BasicResult<()> {
        match command {
            CachedCommand::Noop => Ok(()),
            CachedCommand::Raw(command) => {
                self.execute_command(command.as_ref(), cursor, line_commands)
            }
            CachedCommand::Assignment(compiled) => {
                self.execute_compiled_assignment(compiled.as_ref())
            }
            CachedCommand::MidAssignment(compiled) => {
                self.execute_compiled_mid_assignment(compiled.as_ref())
            }
            CachedCommand::StringCharAssignment {
                target,
                source,
                index,
            } => self.execute_string_char_assignment(target, source, index.as_ref()),
            CachedCommand::DrawRelative2 { x, y } => {
                self.execute_compiled_draw_relative2(x.as_ref(), y.as_ref())
            }
            CachedCommand::If {
                condition,
                then_branch,
                else_branch,
            } => self.execute_cached_if(
                condition.as_ref(),
                then_branch,
                else_branch.as_ref(),
                cursor,
            ),
            CachedCommand::OnGoto { selector, targets } => {
                self.execute_cached_on(selector.as_ref(), targets, false, cursor)
            }
            CachedCommand::OnGosub { selector, targets } => {
                self.execute_cached_on(selector.as_ref(), targets, true, cursor)
            }
            CachedCommand::For(compiled) => self.execute_compiled_for(compiled.as_ref(), cursor),
            CachedCommand::GotoConst { line, target } => {
                self.jump_to_cached_line_checked(*line, target.as_ref(), cursor, false)
            }
            CachedCommand::GosubConst { line, target } => {
                self.gosub_stack.push(Cursor {
                    line_idx: cursor.line_idx,
                    cmd_idx: cursor.cmd_idx + 1,
                });
                self.jump_to_cached_line_checked(*line, target.as_ref(), cursor, true)
            }
            CachedCommand::Return => {
                let ret = self
                    .gosub_stack
                    .pop()
                    .ok_or_else(|| self.err(ErrorCode::ReturnWithoutGosub))?;
                if !self.if_stack.is_empty() {
                    self.reconcile_if_stack_for_jump(&ret)?;
                }
                *cursor = ret;
                Ok(())
            }
            CachedCommand::Next(var) => self.execute_next_cached(var.as_deref(), cursor),
            CachedCommand::While(condition) => {
                self.execute_compiled_while(condition.clone(), cursor)
            }
            CachedCommand::Wend => self.execute_wend(cursor),
        }
    }

    fn execute_cached_if(
        &mut self,
        condition: &Expr,
        then_branch: &CachedIfBranch,
        else_branch: Option<&CachedIfBranch>,
        cursor: &mut Cursor,
    ) -> BasicResult<()> {
        let selected = if eval_compiled_number(self, condition).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })? != 0.0
        {
            Some(then_branch)
        } else {
            else_branch
        };
        let Some(branch) = selected else {
            return Ok(());
        };
        match branch {
            CachedIfBranch::Line(line) => self.jump_to_line(*line, cursor),
            CachedIfBranch::Commands(commands) => {
                self.execute_cached_inline_commands(commands, cursor)
            }
        }
    }

    fn execute_cached_on(
        &mut self,
        selector: &Expr,
        targets: &[i32],
        gosub: bool,
        cursor: &mut Cursor,
    ) -> BasicResult<()> {
        let selector = eval_compiled_number(self, selector).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })? as i32;
        if selector <= 0 || selector as usize > targets.len() {
            return Ok(());
        }
        let target = targets[selector as usize - 1];
        if gosub {
            self.gosub_stack.push(Cursor {
                line_idx: cursor.line_idx,
                cmd_idx: cursor.cmd_idx + 1,
            });
            return self.jump_to_line_checked(target, cursor, true);
        }
        self.jump_to_line_checked(target, cursor, false)
    }

    fn execute_cached_inline_commands(
        &mut self,
        commands: &[Rc<CachedCommand>],
        cursor: &mut Cursor,
    ) -> BasicResult<()> {
        for command in commands {
            let before = cursor.clone();
            self.execute_cached_command(command.as_ref(), cursor, &[])?;
            if self.repeat_current_command
                || *cursor != before
                || self.end_requested
                || self.stopped_cursor.is_some()
                || self.function_return_requested
                || self.sub_return_requested
            {
                break;
            }
        }
        Ok(())
    }

    fn execute_print(&mut self, args: &str) -> BasicResult<()> {
        if args.trim().is_empty() {
            self.write_line("");
            return Ok(());
        }
        if args.trim_start().to_ascii_uppercase().starts_with("USING") {
            return self.execute_print_using(args.trim_start()[5..].trim());
        }
        let mut item = String::new();
        let mut depth = 0i32;
        let mut in_string = false;
        let mut newline = true;
        for ch in args.chars().chain(std::iter::once('\n')) {
            if ch == '"' {
                in_string = !in_string;
                item.push(ch);
                continue;
            }
            if !in_string {
                match ch {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    ';' if depth == 0 => {
                        if self.print_item(item.trim())? {
                            self.write(" ");
                        }
                        if self.end_requested
                            || self.function_return_requested
                            || self.sub_return_requested
                        {
                            return Ok(());
                        }
                        item.clear();
                        newline = false;
                        continue;
                    }
                    ',' if depth == 0 => {
                        self.print_item(item.trim())?;
                        if self.end_requested
                            || self.function_return_requested
                            || self.sub_return_requested
                        {
                            return Ok(());
                        }
                        self.write("\t");
                        item.clear();
                        newline = false;
                        continue;
                    }
                    '\n' if depth == 0 => {
                        self.print_item(item.trim())?;
                        if self.end_requested
                            || self.function_return_requested
                            || self.sub_return_requested
                        {
                            return Ok(());
                        }
                        break;
                    }
                    _ => {}
                }
            }
            item.push(ch);
        }
        if depth != 0 || in_string {
            return Err(self.err(ErrorCode::Syntax));
        }
        if !args.trim_end().ends_with(';') && !args.trim_end().ends_with(',') {
            newline = true;
        }
        if newline {
            self.write_line("");
        } else {
            self.flush_stream_output();
        }
        Ok(())
    }

    fn print_item(&mut self, item: &str) -> BasicResult<bool> {
        if item.is_empty() {
            return Ok(false);
        }
        if let Some(inner) = whole_function_argument(item, "SPC") {
            let count = self.eval_number(inner)?.max(0.0) as usize;
            self.write(&" ".repeat(count));
            return Ok(false);
        }
        if let Some(inner) = whole_function_argument(item, "TAB") {
            let target = self.eval_number(inner)?.max(0.0) as usize;
            let target_col = target.saturating_sub(1);
            if target_col > self.output_col {
                self.write(&" ".repeat(target_col - self.output_col));
            }
            return Ok(false);
        }
        let value = self.eval_value(item)?;
        if self.end_requested || self.function_return_requested || self.sub_return_requested {
            return Ok(false);
        }
        match value {
            Value::Number(n) => {
                self.write(&format_basic_number(n));
                Ok(true)
            }
            Value::Str(s) => {
                self.write(&s);
                Ok(false)
            }
            Value::ArrayRef(_) => Err(self.err(ErrorCode::TypeMismatch)),
        }
    }

    fn execute_assignment(&mut self, source: &str) -> BasicResult<()> {
        if contains_double_equal_top_level(source) {
            return Err(self.err(ErrorCode::Syntax));
        }
        if source
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("MID$(")
        {
            return self.execute_assignment_interpreted(source);
        }
        let compiled = self
            .compiled_assignment(source)
            .map_err(|e| self.with_current_line(e))?;
        self.execute_compiled_assignment(compiled.as_ref())
    }

    fn execute_compiled_assignment(&mut self, compiled: &CompiledAssignment) -> BasicResult<()> {
        let value = (if compiled.rhs_is_string {
            eval_compiled(self, &compiled.rhs)
        } else {
            eval_compiled_number(self, &compiled.rhs).map(Value::number)
        })
        .map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })?;
        for target in &compiled.targets {
            if matches!(value, Value::ArrayRef(_))
                && self
                    .active_function_name()
                    .is_some_and(|name| target.name().eq_ignore_ascii_case(name))
            {
                self.return_value_for_active_function(target.name(), &value);
                continue;
            }
            self.assign_compiled_lvalue(target, value.clone())?;
            self.return_value_for_active_function(target.name(), &value);
        }
        Ok(())
    }

    fn execute_compiled_mid_assignment(
        &mut self,
        compiled: &CompiledMidAssignment,
    ) -> BasicResult<()> {
        let replacement = eval_compiled(self, &compiled.rhs)
            .and_then(|value| value.into_string())
            .map_err(|mut e| {
                if e.line.is_none() {
                    e.line = self.current_line;
                }
                e
            })?;
        let start = eval_compiled_number(self, &compiled.start).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })? as isize;
        if start < 1 {
            return Err(self.err(ErrorCode::OutOfBounds));
        }
        let start = (start - 1) as usize;
        let count = compiled
            .count
            .as_ref()
            .map(|expr| {
                let len = eval_compiled_number(self, expr).map_err(|mut e| {
                    if e.line.is_none() {
                        e.line = self.current_line;
                    }
                    e
                })? as isize;
                if len < 0 {
                    return Err(self.err(ErrorCode::OutOfBounds));
                }
                Ok(len as usize)
            })
            .transpose()?;
        self.replace_mid_string_variable(&compiled.target, start, count, &replacement);
        Ok(())
    }

    fn execute_compiled_draw_relative2(&mut self, x: &Expr, y: &Expr) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let x = eval_compiled_number(self, x).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })?;
        let y = eval_compiled_number(self, y).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })?;
        self.graphics
            .draw_to(self.graphics.xpos() + x, self.graphics.ypos() + y, None);
        self.refresh_graphics_window()
    }

    fn execute_string_char_assignment(
        &mut self,
        target: &str,
        source: &str,
        index: &Expr,
    ) -> BasicResult<()> {
        let start = eval_compiled_number(self, index).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })? as isize
            - 1;
        let start = start.max(0) as usize;
        let value = self
            .string_variables
            .get(source)
            .map(|text| basic_string_slice(text, start, Some(1)))
            .unwrap_or_default();
        self.string_variables.insert(target.to_string(), value);
        Ok(())
    }

    fn execute_assignment_interpreted(&mut self, source: &str) -> BasicResult<()> {
        if let Some((targets, rhs)) = split_assignment_targets_and_rhs(source) {
            if targets
                .iter()
                .any(|target| assignment_target_is_reserved_function(target))
            {
                return Err(self.err(ErrorCode::Syntax));
            }
            let string_assignment = targets.iter().any(|lhs| assignment_target_is_string(lhs));
            let value = if string_assignment {
                self.eval_value(rhs)?
            } else {
                Value::number(self.eval_number(rhs)?)
            };
            for lhs in &targets {
                if matches!(value, Value::ArrayRef(_))
                    && self
                        .active_function_name()
                        .is_some_and(|name| lhs.trim().eq_ignore_ascii_case(name))
                {
                    self.return_value_for_active_function(lhs, &value);
                    continue;
                }
                self.assign(lhs, value.clone())?;
                self.return_value_for_active_function(lhs, &value);
            }
            return Ok(());
        }
        let Some(pos) = find_assignment_equal(source) else {
            return Err(self.err(ErrorCode::Syntax));
        };
        let lhs = source[..pos].trim();
        if assignment_target_is_reserved_function(lhs) {
            return Err(self.err(ErrorCode::Syntax));
        }
        let rhs = source[pos + 1..].trim();
        let value = if assignment_target_is_string(lhs) {
            self.eval_value(rhs)?
        } else {
            Value::number(self.eval_number(rhs)?)
        };
        self.assign(lhs, value.clone())?;
        self.return_value_for_active_function(lhs, &value);
        Ok(())
    }

    fn compiled_assignment(&mut self, source: &str) -> BasicResult<Rc<CompiledAssignment>> {
        let key = source.trim();
        if let Some(compiled) = self.assignment_cache.get(key) {
            return Ok(compiled.clone());
        }
        let compiled = Rc::new(compile_assignment_statement(key)?);
        self.assignment_cache
            .insert(key.to_string(), compiled.clone());
        Ok(compiled)
    }

    fn execute_print_using(&mut self, args: &str) -> BasicResult<()> {
        let Some((fmt_expr, tail)) = split_first_top_level(args, ';') else {
            return Err(self.err(ErrorCode::Syntax));
        };
        let fmt = self.eval_value(fmt_expr.trim())?.into_string()?;
        if !valid_using_format(&fmt) {
            return Err(self.err(ErrorCode::UsingFormat));
        }
        let items = split_print_items(tail);
        let newline = !tail.trim_end().ends_with(';') && !tail.trim_end().ends_with(',');
        for (item, sep) in items {
            if item.trim().is_empty() {
                if sep == Some(',') {
                    self.write("\t");
                }
                continue;
            }
            let value = self.eval_value(item.trim())?;
            match value {
                Value::Number(n) => self.write(&format_using_simple(n, &fmt)),
                Value::Str(s) => self.write(&s),
                Value::ArrayRef(_) => return Err(self.err(ErrorCode::TypeMismatch)),
            }
            match sep {
                Some(',') => self.write("\t"),
                Some(';') | None => {}
                _ => {}
            }
        }
        if newline {
            self.write_line("");
        } else {
            self.flush_stream_output();
        }
        Ok(())
    }

    fn execute_input(&mut self, args: &str, cursor: &Cursor) -> BasicResult<()> {
        let mut body = args.trim();
        if body.starts_with('"') {
            let Some(end) = body[1..].find('"') else {
                return Err(self.err(ErrorCode::Syntax));
            };
            let prompt = &body[1..end + 1];
            self.write(prompt);
            body = body[end + 2..].trim_start();
            if body.starts_with(';') {
                self.write("? ");
                body = body[1..].trim_start();
            } else if body.starts_with(',') {
                body = body[1..].trim_start();
            }
        } else {
            self.write("? ");
        }
        print!("{}", self.take_output());
        let _ = io::stdout().flush();
        let mut line = String::new();
        {
            let _runtime_raw_suspend = console::suspend_runtime_raw_mode().ok();
            if let Err(err) = io::stdin().read_line(&mut line) {
                if err.kind() == io::ErrorKind::Interrupted {
                    return self.check_user_interrupt(cursor);
                }
                return Err(self
                    .err(ErrorCode::InvalidValue)
                    .with_detail(err.to_string()));
            }
        }
        self.check_user_interrupt(cursor)?;
        let values = split_top_level(line.trim_end_matches(&['\r', '\n'][..]), &[',']);
        let targets = split_arguments(body);
        if values.len() < targets.len() {
            return Err(self.err(ErrorCode::InvalidValue));
        }
        for (target, raw) in targets.into_iter().zip(values.into_iter()) {
            let value = if target.trim().ends_with('$') {
                Value::string(raw.trim_matches('"').to_string())
            } else {
                Value::number(
                    raw.trim()
                        .parse::<f64>()
                        .map_err(|_| self.err(ErrorCode::TypeMismatch))?,
                )
            };
            self.assign(&target, value)?;
        }
        Ok(())
    }

    fn execute_line_input(&mut self, args: &str, cursor: &Cursor) -> BasicResult<()> {
        let mut body = args.trim();
        let mut prompt: Option<&str> = None;
        let mut prompt_question = false;
        if body.starts_with('"') {
            let Some(end) = body[1..].find('"') else {
                return Err(self.err(ErrorCode::Syntax));
            };
            prompt = Some(&body[1..end + 1]);
            body = body[end + 2..].trim_start();
            if body.starts_with(';') {
                prompt_question = true;
                body = body[1..].trim_start();
            } else if body.starts_with(',') {
                body = body[1..].trim_start();
            }
        } else {
            prompt_question = true;
        }

        let targets = split_arguments(body);
        if targets.len() != 1 || targets[0].trim().is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }
        let target = targets[0].trim();
        if !assignment_target_is_string(target) {
            return Err(self.err(ErrorCode::TypeMismatch));
        }

        if let Some(prompt) = prompt {
            self.write(prompt);
        }
        if prompt_question {
            self.write("? ");
        }
        print!("{}", self.take_output());
        let _ = io::stdout().flush();

        let mut line = String::new();
        {
            let _runtime_raw_suspend = console::suspend_runtime_raw_mode().ok();
            if let Err(err) = io::stdin().read_line(&mut line) {
                if err.kind() == io::ErrorKind::Interrupted {
                    return self.check_user_interrupt(cursor);
                }
                return Err(self
                    .err(ErrorCode::InvalidValue)
                    .with_detail(err.to_string()));
            }
        }
        self.check_user_interrupt(cursor)?;
        let value = line.trim_end_matches(&['\r', '\n'][..]).to_string();
        self.assign(target, Value::string(value))
    }

    fn assign(&mut self, lhs: &str, value: Value) -> BasicResult<()> {
        if let Some((canonical, display)) = assignment_identifier_case(lhs) {
            self.identifier_case.entry(canonical).or_insert(display);
        }
        let lhs = lhs.trim().to_ascii_uppercase();
        if lhs.starts_with("MID$(") && lhs.ends_with(')') {
            return self.assign_mid_string(&lhs, value);
        }
        if let Some(open) = lhs.find('(') {
            let close = lhs.rfind(')').ok_or_else(|| self.err(ErrorCode::Syntax))?;
            let name = lhs[..open].trim().to_string();
            let indexes = split_arguments(&lhs[open + 1..close])
                .into_iter()
                .map(|arg| self.eval_number(&arg).map(|n| n as i32))
                .collect::<BasicResult<Vec<_>>>()?;
            let indexes = self.normalize_array_indexes_for_name(&name, indexes)?;
            if !self.arrays.contains_key(&name) {
                let dims = vec![10; indexes.len()];
                self.arrays
                    .insert(name.clone(), ArrayValue::new(&name, dims));
            }
            let array = self.arrays.get_mut(&name).unwrap();
            if array.is_string() != matches!(value, Value::Str(_)) {
                return Err(self.err(ErrorCode::TypeMismatch));
            }
            array.set(&indexes, value).map_err(|mut e| {
                if e.line.is_none() {
                    e.line = self.current_line;
                }
                e
            })?;
            Ok(())
        } else {
            if lhs.ends_with('$') != matches!(value, Value::Str(_)) {
                return Err(self.err(ErrorCode::TypeMismatch));
            }
            match value {
                Value::Number(n) => {
                    self.numeric_variables.insert(lhs, n);
                }
                Value::Str(s) => {
                    self.string_variables.insert(lhs, s);
                }
                Value::ArrayRef(_) => return Err(self.err(ErrorCode::TypeMismatch)),
            }
            Ok(())
        }
    }

    fn assign_compiled_lvalue(&mut self, target: &CompiledLValue, value: Value) -> BasicResult<()> {
        match target {
            CompiledLValue::Scalar { name, is_string } => {
                if *is_string != matches!(value, Value::Str(_)) {
                    return Err(self.err(ErrorCode::TypeMismatch));
                }
                match value {
                    Value::Number(n) => {
                        if let Some(slot) = self.numeric_variables.get_mut(name) {
                            *slot = n;
                        } else {
                            self.numeric_variables.insert(name.clone(), n);
                        }
                    }
                    Value::Str(s) => {
                        if let Some(slot) = self.string_variables.get_mut(name) {
                            *slot = s;
                        } else {
                            self.string_variables.insert(name.clone(), s);
                        }
                    }
                    Value::ArrayRef(_) => return Err(self.err(ErrorCode::TypeMismatch)),
                }
                Ok(())
            }
            CompiledLValue::Array {
                name,
                indexes,
                is_string,
            } => match indexes.len() {
                1 => {
                    let raw = [eval_compiled_number(self, &indexes[0])? as i32];
                    self.assign_compiled_array_value(name, &raw, *is_string, value)
                }
                2 => {
                    let raw = [
                        eval_compiled_number(self, &indexes[0])? as i32,
                        eval_compiled_number(self, &indexes[1])? as i32,
                    ];
                    self.assign_compiled_array_value(name, &raw, *is_string, value)
                }
                _ => {
                    let raw = indexes
                        .iter()
                        .map(|expr| eval_compiled_number(self, expr).map(|n| n as i32))
                        .collect::<BasicResult<Vec<_>>>()?;
                    self.assign_compiled_array_value(name, &raw, *is_string, value)
                }
            },
        }
    }

    fn assign_compiled_array_value(
        &mut self,
        name: &str,
        raw_indexes: &[i32],
        is_string: bool,
        value: Value,
    ) -> BasicResult<()> {
        if self
            .arrays
            .get(name)
            .is_some_and(|array| array.dims.len() == raw_indexes.len())
        {
            let array = self.arrays.get_mut(name).unwrap();
            if array.is_string() != is_string || is_string != matches!(value, Value::Str(_)) {
                return Err(self.err(ErrorCode::TypeMismatch));
            }
            if raw_indexes.len() == 1 {
                return array.set_direct_1d(raw_indexes[0], value).map_err(|mut e| {
                    if e.line.is_none() {
                        e.line = self.current_line;
                    }
                    e
                });
            }
            return array.set(raw_indexes, value).map_err(|mut e| {
                if e.line.is_none() {
                    e.line = self.current_line;
                }
                e
            });
        }
        let indexes = self.normalize_array_indexes_for_name(name, raw_indexes.to_vec())?;
        if !self.arrays.contains_key(name) {
            let dims = vec![10; indexes.len()];
            self.arrays
                .insert(name.to_string(), ArrayValue::new(name, dims));
        }
        let array = self.arrays.get_mut(name).unwrap();
        if array.is_string() != is_string || is_string != matches!(value, Value::Str(_)) {
            return Err(self.err(ErrorCode::TypeMismatch));
        }
        array.set(&indexes, value).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })
    }

    fn assign_mid_string(&mut self, lhs: &str, value: Value) -> BasicResult<()> {
        let replacement = value.into_string()?;
        let inner = &lhs[5..lhs.len() - 1];
        let args = split_arguments(inner);
        if args.len() < 2 || args.len() > 3 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let target = args[0].trim();
        let start = self.eval_number(&args[1])? as isize;
        if start < 1 {
            return Err(self.err(ErrorCode::OutOfBounds));
        }
        let start = (start - 1) as usize;
        let count_arg = if args.len() == 3 {
            let len = self.eval_number(&args[2])? as isize;
            if len < 0 {
                return Err(self.err(ErrorCode::OutOfBounds));
            }
            Some(len as usize)
        } else {
            None
        };

        if let Some(upper_target) = simple_string_variable_name(target) {
            self.replace_mid_string_variable(&upper_target, start, count_arg, &replacement);
            return Ok(());
        }

        let original = self.get_lvalue(target)?.into_string()?;
        let mut chars: Vec<char> = original.chars().collect();
        if start >= chars.len() {
            return Ok(());
        }
        let max_count = chars.len() - start;
        let count = count_arg.unwrap_or(max_count).min(max_count);
        for (offset, ch) in replacement.chars().take(count).enumerate() {
            chars[start + offset] = ch;
        }
        self.assign(target, Value::string(chars.into_iter().collect::<String>()))
    }

    fn replace_mid_string_variable(
        &mut self,
        target: &str,
        start: usize,
        count: Option<usize>,
        replacement: &str,
    ) {
        let Some(original) = self.string_variables.get_mut(target) else {
            return;
        };
        if !replace_mid_ascii(original, start, count, replacement) {
            replace_mid_general(original, start, count, replacement);
        }
    }

    fn execute_swap(&mut self, args: &str) -> BasicResult<()> {
        let parts = split_arguments(args);
        if parts.len() != 2 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let left = parts[0].trim();
        let right = parts[1].trim();
        let left_value = self.get_lvalue(left)?;
        let right_value = self.get_lvalue(right)?;
        self.assign(left, right_value)?;
        self.assign(right, left_value)?;
        Ok(())
    }

    fn get_lvalue(&mut self, target: &str) -> BasicResult<Value> {
        let target = target.trim();
        if target.is_empty() {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let upper = target.to_ascii_uppercase();
        if let Some(open) = upper.find('(') {
            if !upper.ends_with(')') {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            let name = upper[..open].trim();
            if !is_basic_identifier(name) {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            let indexes = split_arguments(&upper[open + 1..upper.len() - 1])
                .into_iter()
                .map(|arg| self.eval_number(&arg).map(|n| n as i32))
                .collect::<BasicResult<Vec<_>>>()?;
            return self.get_array_value(name, &indexes).map_err(|mut e| {
                if e.line.is_none() {
                    e.line = self.current_line;
                }
                e
            });
        }
        if !is_basic_identifier(&upper) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        if !self.numeric_variables.contains_key(&upper)
            && !self.string_variables.contains_key(&upper)
            && self.arrays.contains_key(&upper)
        {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        self.get_variable(&upper)
    }

    fn normalize_array_indexes_for_name(
        &self,
        name: &str,
        indexes: Vec<i32>,
    ) -> BasicResult<Vec<i32>> {
        if indexes.len() == 2 {
            if let Some(array) = self.arrays.get(name) {
                if array.dims.len() == 1 {
                    if indexes[1] == self.mat_base {
                        return Ok(vec![indexes[0]]);
                    }
                    if indexes[0] == self.mat_base {
                        return Ok(vec![indexes[1]]);
                    }
                    if indexes[0] < self.mat_base && indexes[1] < self.mat_base {
                        return Ok(vec![indexes[0]]);
                    }
                }
            }
        }
        Ok(indexes)
    }

    fn execute_if(
        &mut self,
        command: &str,
        cursor: &mut Cursor,
        line_commands: &[String],
    ) -> BasicResult<()> {
        let upper = command.to_ascii_uppercase();
        let (cond, rest) = if let Some((then_pos, then_end)) = find_then_keyword(&upper) {
            (command[2..then_pos].trim(), command[then_end..].trim())
        } else if let Some(goto_pos) = find_keyword_after_if(&upper, " GOTO ") {
            (command[2..goto_pos].trim(), command[goto_pos + 1..].trim())
        } else {
            return Err(self.err(ErrorCode::Syntax));
        };
        if rest.is_empty() {
            if self.eval_number(cond)? != 0.0 {
                self.if_stack.push(Cursor {
                    line_idx: cursor.line_idx,
                    cmd_idx: cursor.cmd_idx,
                });
                return Ok(());
            }
            return self.jump_to_next_if_branch(cursor);
        }
        let (then_part, else_part) = split_else(rest);
        let selected = if self.eval_number(cond)? != 0.0 {
            then_part
        } else {
            else_part.unwrap_or("")
        };
        if selected.is_empty() {
            return Ok(());
        }
        if let Ok(line) = selected.trim().parse::<i32>() {
            return self.jump_to_line(line, cursor);
        }
        let subcommands = split_commands(selected);
        for (idx, sub) in subcommands.iter().enumerate() {
            let before = cursor.clone();
            if idx + 1 < subcommands.len() && first_word_is(sub, "GOSUB") {
                self.execute_inline_gosub(sub[5..].trim())?;
                continue;
            }
            self.execute_command(&sub, cursor, line_commands)?;
            if self.repeat_current_command
                || *cursor != before
                || self.end_requested
                || self.stopped_cursor.is_some()
                || self.function_return_requested
                || self.sub_return_requested
            {
                break;
            }
        }
        Ok(())
    }

    fn execute_elseif(
        &mut self,
        command: &str,
        cursor: &mut Cursor,
        line_commands: &[String],
    ) -> BasicResult<()> {
        let current = Cursor {
            line_idx: cursor.line_idx,
            cmd_idx: cursor.cmd_idx,
        };
        if self.pending_if_branch.as_ref() != Some(&current) {
            return self.skip_after_matching_end_if(cursor);
        }
        self.pending_if_branch = None;

        let upper = command.to_ascii_uppercase();
        let Some((then_pos, then_end)) = find_then_keyword(&upper) else {
            return Err(self.err(ErrorCode::Syntax));
        };
        let cond = command[6..then_pos].trim();
        let rest = command[then_end..].trim();
        if self.eval_number(cond)? == 0.0 {
            return self.jump_to_next_if_branch(cursor);
        }
        if rest.is_empty() {
            self.if_stack.push(Cursor {
                line_idx: cursor.line_idx,
                cmd_idx: cursor.cmd_idx,
            });
            return Ok(());
        }
        if let Ok(line) = rest.parse::<i32>() {
            return self.jump_to_line(line, cursor);
        }
        let subcommands = split_commands(rest);
        for (idx, sub) in subcommands.iter().enumerate() {
            let before = cursor.clone();
            if idx + 1 < subcommands.len() && first_word_is(sub, "GOSUB") {
                self.execute_inline_gosub(sub[5..].trim())?;
                continue;
            }
            self.execute_command(&sub, cursor, line_commands)?;
            if self.repeat_current_command
                || *cursor != before
                || self.end_requested
                || self.stopped_cursor.is_some()
                || self.function_return_requested
                || self.sub_return_requested
            {
                break;
            }
        }
        Ok(())
    }

    fn execute_else(&mut self, cursor: &mut Cursor) -> BasicResult<()> {
        let current = Cursor {
            line_idx: cursor.line_idx,
            cmd_idx: cursor.cmd_idx,
        };
        if self.pending_if_branch.as_ref() == Some(&current) {
            self.pending_if_branch = None;
            self.if_stack.push(current);
            return Ok(());
        }
        self.skip_after_matching_end_if(cursor)
    }

    fn execute_end_if(&mut self, _cursor: &mut Cursor) -> BasicResult<()> {
        if self.if_stack.pop().is_some() {
            Ok(())
        } else {
            Err(self.err(ErrorCode::EndIfWithoutIf))
        }
    }

    fn jump_to_next_if_branch(&mut self, cursor: &mut Cursor) -> BasicResult<()> {
        let branch = self.find_next_if_branch(cursor)?;
        match branch.kind {
            IfBranchKind::ElseIf | IfBranchKind::Else => {
                self.pending_if_branch = Some(branch.cursor.clone());
                *cursor = branch.cursor;
            }
            IfBranchKind::EndIf => {
                *cursor = Cursor {
                    line_idx: branch.cursor.line_idx,
                    cmd_idx: branch.cursor.cmd_idx + 1,
                };
            }
        }
        Ok(())
    }

    fn skip_after_matching_end_if(&mut self, cursor: &mut Cursor) -> BasicResult<()> {
        *cursor = self.find_after_matching_end_if(cursor)?;
        self.if_stack.pop();
        Ok(())
    }

    fn find_next_if_branch(&self, cursor: &Cursor) -> BasicResult<IfBranch> {
        let lines = self.program.line_numbers();
        let mut depth = 0i32;
        for line_idx in cursor.line_idx..lines.len() {
            let line_no = lines[line_idx];
            let commands = if let Some(cached) = self.command_cache.get(&line_no) {
                cached.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            } else {
                split_commands(self.program.get(line_no).unwrap_or(""))
            };
            let start_cmd = if line_idx == cursor.line_idx {
                cursor.cmd_idx + 1
            } else {
                0
            };
            for (cmd_idx, cmd) in commands.iter().enumerate().skip(start_cmd) {
                let kind = classify_if_branch_command(cmd);
                if is_multiline_if_start(cmd) {
                    depth += 1;
                } else if matches!(kind, Some(IfBranchKind::EndIf)) {
                    if depth == 0 {
                        return Ok(IfBranch {
                            cursor: Cursor { line_idx, cmd_idx },
                            kind: IfBranchKind::EndIf,
                        });
                    }
                    depth -= 1;
                } else if depth == 0 {
                    if let Some(kind @ (IfBranchKind::ElseIf | IfBranchKind::Else)) = kind {
                        return Ok(IfBranch {
                            cursor: Cursor { line_idx, cmd_idx },
                            kind,
                        });
                    }
                }
            }
        }
        Err(self.err(ErrorCode::Syntax))
    }

    fn find_after_matching_end_if(&self, cursor: &Cursor) -> BasicResult<Cursor> {
        let lines = self.program.line_numbers();
        let mut depth = 0i32;
        for line_idx in cursor.line_idx..lines.len() {
            let line_no = lines[line_idx];
            let commands = if let Some(cached) = self.command_cache.get(&line_no) {
                cached.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            } else {
                split_commands(self.program.get(line_no).unwrap_or(""))
            };
            let start_cmd = if line_idx == cursor.line_idx {
                cursor.cmd_idx + 1
            } else {
                0
            };
            for (cmd_idx, cmd) in commands.iter().enumerate().skip(start_cmd) {
                if is_multiline_if_start(cmd) {
                    depth += 1;
                } else if matches!(classify_if_branch_command(cmd), Some(IfBranchKind::EndIf)) {
                    if depth == 0 {
                        return Ok(Cursor {
                            line_idx,
                            cmd_idx: cmd_idx + 1,
                        });
                    }
                    depth -= 1;
                }
            }
        }
        Err(self.err(ErrorCode::Syntax))
    }

    fn execute_on(&mut self, command: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let body = command[2..].trim();
        let upper = body.to_ascii_uppercase();
        let (kind, pos, keyword_len) = if let Some(pos) = upper.find(" GOSUB ") {
            ("GOSUB", pos, 7)
        } else if let Some(pos) = upper.find(" GOTO ") {
            ("GOTO", pos, 6)
        } else {
            return Err(self.err(ErrorCode::Syntax));
        };

        let selector = self.eval_number(body[..pos].trim())? as i32;
        let targets = split_arguments(body[pos + keyword_len..].trim());
        if selector <= 0 || selector as usize > targets.len() {
            return Ok(());
        }

        let target = self.eval_number(&targets[selector as usize - 1])? as i32;
        if kind == "GOSUB" {
            self.gosub_stack.push(Cursor {
                line_idx: cursor.line_idx,
                cmd_idx: cursor.cmd_idx + 1,
            });
            return self.jump_to_line_checked(target, cursor, true);
        }
        self.jump_to_line_checked(target, cursor, false)
    }

    fn execute_on_mouse(&mut self, command: &str) -> BasicResult<()> {
        let body = command[2..].trim();
        let upper = body.to_ascii_uppercase();
        let Some(rest) = upper.strip_prefix("MOUSE") else {
            return Err(self.err(ErrorCode::Syntax));
        };
        let rest = rest.trim_start();
        let Some((event, tail)) = rest.split_once(' ') else {
            return Err(self.err(ErrorCode::Syntax));
        };
        if !is_mouse_event_name(event) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let tail = tail.trim_start();
        if !tail.to_ascii_uppercase().starts_with("GOSUB ") {
            return Err(self.err(ErrorCode::Syntax));
        }
        let target_expr = command[command.len() - tail[6..].len()..].trim();
        if target_expr.is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }
        let target_line = self.eval_number(target_expr)? as i32;
        if target_line == 0 {
            self.mouse_handlers.remove(event);
            return Ok(());
        }
        if self.line_index(target_line).is_none() {
            return Err(self.err(ErrorCode::TargetLineNotFound));
        }
        self.mouse_handlers.insert(event.to_string(), target_line);
        if self.graphics_window.is_some() && self.graphics_window_enabled {
            self.prepare_graphics_window_use_by_current_run()?;
        }
        Ok(())
    }

    fn execute_inline_gosub(&mut self, target_expr: &str) -> BasicResult<()> {
        let line = self.eval_number(target_expr)? as i32;
        let Some(index) = self.line_index(line) else {
            return Err(self.err(ErrorCode::TargetLineNotFound));
        };
        let target = Cursor {
            line_idx: index,
            cmd_idx: 0,
        };
        self.validate_function_jump(line, &target, true)?;
        let saved_current_line = self.current_line;
        let saved_gosub_len = self.gosub_stack.len();
        self.gosub_stack.push(Cursor {
            line_idx: usize::MAX,
            cmd_idx: 0,
        });
        let result = self.run_from(target);
        self.gosub_stack.truncate(saved_gosub_len);
        self.current_line = saved_current_line;
        result?;
        Ok(())
    }

    fn execute_on_error(&mut self, command: &str) -> BasicResult<()> {
        let tail = command[8..].trim();
        let upper = tail.to_ascii_uppercase();
        if upper == "RESUME NEXT" {
            self.error_handler_line = None;
            self.error_resume_next = true;
            return Ok(());
        }
        if upper.starts_with("GOTO") {
            let target = tail[4..].trim();
            let line = self.eval_number(target)? as i32;
            if line == 0 {
                self.error_handler_line = None;
                self.error_resume_next = false;
                return Ok(());
            }
            if self.line_index(line).is_none() {
                return Err(self.err(ErrorCode::TargetLineNotFound));
            }
            if self.function_for_line(line).is_some() || self.sub_for_line(line).is_some() {
                return Err(self.err(ErrorCode::InvalidTargetLine));
            }
            self.error_handler_line = Some(line);
            self.error_resume_next = false;
            return Ok(());
        }
        Err(self.err(ErrorCode::Syntax))
    }

    fn execute_error(&mut self, arg: &str) -> BasicResult<()> {
        let number = if arg.trim().is_empty() {
            0
        } else {
            self.eval_number(arg)? as i32
        };
        let err = ErrorCode::from_number(number)
            .map(|code| self.err(code))
            .unwrap_or_else(|| {
                self.err(ErrorCode::InvalidValue)
                    .with_detail(format!("Error {number}"))
            });
        Err(err)
    }

    fn execute_resume(&mut self, arg: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let Some(state) = self.last_error.clone() else {
            return Err(BasicError::new(ErrorCode::HandlerError)
                .with_detail("Error in error handler: RESUME without ERROR."));
        };
        let arg = arg.trim();
        if arg.is_empty() || arg == "0" {
            *cursor = state.retry;
        } else if arg.eq_ignore_ascii_case("NEXT") {
            *cursor = state.next;
        } else {
            let line = self.eval_number(arg)? as i32;
            self.jump_to_line_for_resume(line, cursor)?;
        }
        self.handling_error = false;
        self.last_error = None;
        Ok(())
    }

    fn execute_for(&mut self, args: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let compiled = compile_for_statement(args).map_err(|e| self.with_current_line(e))?;
        self.execute_compiled_for(&compiled, cursor)
    }

    fn execute_compiled_for(
        &mut self,
        compiled: &CompiledFor,
        cursor: &mut Cursor,
    ) -> BasicResult<()> {
        let start = eval_compiled_number(self, &compiled.start).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })?;
        let end = eval_compiled_number(self, &compiled.end).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })?;
        let step = if let Some(step) = &compiled.step {
            eval_compiled_number(self, step).map_err(|mut e| {
                if e.line.is_none() {
                    e.line = self.current_line;
                }
                e
            })?
        } else {
            1.0
        };
        self.numeric_variables.insert(compiled.var.clone(), start);
        let enters = if step >= 0.0 {
            start <= end
        } else {
            start >= end
        };
        if !enters {
            *cursor = self.find_after_matching_next(cursor)?;
            return Ok(());
        }
        self.for_stack.push(ForFrame {
            var: compiled.var.clone(),
            end,
            step,
            resume: Cursor {
                line_idx: cursor.line_idx,
                cmd_idx: cursor.cmd_idx + 1,
            },
        });
        Ok(())
    }

    fn find_after_matching_next(&self, cursor: &Cursor) -> BasicResult<Cursor> {
        if let Some(target) = self.next_after_for_cache.get(cursor) {
            return Ok(target.clone());
        }
        let lines = self.program.line_numbers();
        let mut depth = 0i32;
        for line_idx in cursor.line_idx..lines.len() {
            let line_no = lines[line_idx];
            let commands = split_commands(self.program.get(line_no).unwrap_or(""));
            let start_cmd = if line_idx == cursor.line_idx {
                cursor.cmd_idx + 1
            } else {
                0
            };
            for (cmd_idx, cmd) in commands.iter().enumerate().skip(start_cmd) {
                let first = cmd
                    .trim_start()
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                if first == "FOR" {
                    depth += 1;
                } else if first == "NEXT" {
                    if depth == 0 {
                        return Ok(Cursor {
                            line_idx,
                            cmd_idx: cmd_idx + 1,
                        });
                    }
                    depth -= 1;
                }
            }
        }
        Err(self.err(ErrorCode::ForWithoutNext))
    }

    fn execute_next(&mut self, arg: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let requested = arg.trim().to_ascii_uppercase();
        let frame_index = if requested.is_empty() {
            self.for_stack.len().checked_sub(1)
        } else {
            self.for_stack
                .iter()
                .rposition(|frame| frame.var == requested)
        };
        let Some(frame_index) = frame_index else {
            return Err(self.err(ErrorCode::NextWithoutFor));
        };
        if frame_index + 1 < self.for_stack.len() {
            self.for_stack.truncate(frame_index + 1);
        }
        let Some(frame) = self.for_stack.last().cloned() else {
            return Err(self.err(ErrorCode::NextWithoutFor));
        };
        let current = self.get_variable(&frame.var)?.as_number()? + frame.step;
        self.numeric_variables.insert(frame.var.clone(), current);
        let keep = if frame.step >= 0.0 {
            current <= frame.end
        } else {
            current >= frame.end
        };
        if keep {
            if frame.resume == *cursor {
                self.repeat_current_command = true;
            } else {
                *cursor = frame.resume;
            }
        } else {
            self.for_stack.pop();
        }
        Ok(())
    }

    fn execute_next_cached(
        &mut self,
        requested: Option<&str>,
        cursor: &mut Cursor,
    ) -> BasicResult<()> {
        let frame_index = match requested.filter(|name| !name.is_empty()) {
            None => self.for_stack.len().checked_sub(1),
            Some(name) => self.for_stack.iter().rposition(|frame| frame.var == name),
        };
        let Some(frame_index) = frame_index else {
            return Err(self.err(ErrorCode::NextWithoutFor));
        };
        if frame_index + 1 < self.for_stack.len() {
            self.for_stack.truncate(frame_index + 1);
        }
        let Some(frame) = self.for_stack.last().cloned() else {
            return Err(self.err(ErrorCode::NextWithoutFor));
        };
        let current = self.get_variable(&frame.var)?.as_number()? + frame.step;
        self.numeric_variables.insert(frame.var.clone(), current);
        let keep = if frame.step >= 0.0 {
            current <= frame.end
        } else {
            current >= frame.end
        };
        if keep {
            if frame.resume == *cursor {
                self.repeat_current_command = true;
            } else {
                *cursor = frame.resume;
            }
        } else {
            self.for_stack.pop();
        }
        Ok(())
    }

    fn execute_exit_for(&mut self, cursor: &mut Cursor) -> BasicResult<()> {
        let Some(frame) = self.for_stack.pop() else {
            return Err(self.err(ErrorCode::NextWithoutFor));
        };
        let target = self.find_after_matching_next(cursor)?;
        self.for_stack
            .retain(|other| !cursor_in_exited_block(&other.resume, &frame.resume, &target));
        self.while_stack
            .retain(|other| !cursor_in_exited_block(&other.resume, &frame.resume, &target));
        self.reconcile_if_stack_for_jump(&target)?;
        *cursor = target;
        Ok(())
    }

    fn execute_while(&mut self, expr: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let compiled = if let Some(compiled) = self.expression_cache.get(expr.trim()) {
            compiled.clone()
        } else {
            let compiled =
                Rc::new(compile_expression(expr).map_err(|e| self.with_current_line(e))?);
            self.expression_cache
                .insert(expr.trim().to_string(), compiled.clone());
            compiled
        };
        self.execute_compiled_while(compiled, cursor)
    }

    fn execute_compiled_while(&mut self, expr: Rc<Expr>, cursor: &mut Cursor) -> BasicResult<()> {
        let header = Cursor {
            line_idx: cursor.line_idx,
            cmd_idx: cursor.cmd_idx,
        };
        if eval_compiled_number(self, expr.as_ref()).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })? != 0.0
        {
            if self
                .while_stack
                .last()
                .is_some_and(|frame| frame.header == header)
            {
                return Ok(());
            }
            self.while_stack.push(WhileFrame {
                expr,
                header,
                resume: Cursor {
                    line_idx: cursor.line_idx,
                    cmd_idx: cursor.cmd_idx + 1,
                },
            });
        } else {
            *cursor = self.find_after_matching_wend(cursor)?;
        }
        Ok(())
    }

    fn find_after_matching_wend(&self, cursor: &Cursor) -> BasicResult<Cursor> {
        if let Some(target) = self.wend_after_while_cache.get(cursor) {
            return Ok(target.clone());
        }
        let lines = self.program.line_numbers();
        let mut depth = 0i32;
        for line_idx in cursor.line_idx..lines.len() {
            let line_no = lines[line_idx];
            let commands = if let Some(cached) = self.command_cache.get(&line_no) {
                cached.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            } else {
                split_commands(self.program.get(line_no).unwrap_or(""))
            };
            let start_cmd = if line_idx == cursor.line_idx {
                cursor.cmd_idx + 1
            } else {
                0
            };
            for (cmd_idx, cmd) in commands.iter().enumerate().skip(start_cmd) {
                let first = cmd
                    .trim_start()
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                if first == "WHILE" {
                    depth += 1;
                } else if first == "WEND" {
                    if depth == 0 {
                        return Ok(Cursor {
                            line_idx,
                            cmd_idx: cmd_idx + 1,
                        });
                    }
                    depth -= 1;
                }
            }
        }
        Err(self.err(ErrorCode::WhileWithoutWend))
    }

    fn execute_wend(&mut self, cursor: &mut Cursor) -> BasicResult<()> {
        let Some(frame) = self.while_stack.last().cloned() else {
            return Err(self.err(ErrorCode::WendWithoutWhile));
        };
        if eval_compiled_number(self, frame.expr.as_ref()).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })? != 0.0
        {
            *cursor = frame.resume;
        } else {
            self.while_stack.pop();
        }
        Ok(())
    }

    fn execute_exit_while(&mut self, cursor: &mut Cursor) -> BasicResult<()> {
        let Some(frame) = self.while_stack.pop() else {
            return Err(self.err(ErrorCode::WendWithoutWhile));
        };
        let target = self.find_after_matching_wend(cursor)?;
        self.for_stack
            .retain(|other| !cursor_in_exited_block(&other.resume, &frame.resume, &target));
        self.while_stack
            .retain(|other| !cursor_in_exited_block(&other.resume, &frame.resume, &target));
        self.reconcile_if_stack_for_jump(&target)?;
        *cursor = target;
        Ok(())
    }

    fn execute_dim(&mut self, args: &str) -> BasicResult<()> {
        for spec in split_arguments(args) {
            if spec.trim().is_empty() {
                continue;
            }
            let open = spec.find('(').ok_or_else(|| self.err(ErrorCode::Syntax))?;
            let close = spec.rfind(')').ok_or_else(|| self.err(ErrorCode::Syntax))?;
            let raw_name = spec[..open].trim();
            if is_basic_identifier(raw_name) {
                self.identifier_case
                    .entry(raw_name.to_ascii_uppercase())
                    .or_insert_with(|| raw_name.to_string());
            }
            let name = raw_name.to_ascii_uppercase();
            let dims = split_arguments(&spec[open + 1..close])
                .into_iter()
                .map(|arg| {
                    if arg.trim().is_empty() {
                        return Err(self.err(ErrorCode::UndefinedIndex));
                    }
                    let n = self.eval_number(&arg)?;
                    if n < 0.0 {
                        return Err(self.err(ErrorCode::InvalidValue));
                    }
                    Ok(n as usize)
                })
                .collect::<BasicResult<Vec<_>>>()?;
            self.arrays
                .insert(name.clone(), ArrayValue::new(&name, dims));
        }
        Ok(())
    }

    fn execute_redim(&mut self, args: &str) -> BasicResult<()> {
        for spec in split_arguments(args) {
            if spec.trim().is_empty() {
                continue;
            }
            let open = spec.find('(').ok_or_else(|| self.err(ErrorCode::Syntax))?;
            let close = spec.rfind(')').ok_or_else(|| self.err(ErrorCode::Syntax))?;
            let raw_name = spec[..open].trim();
            if is_basic_identifier(raw_name) {
                self.identifier_case
                    .entry(raw_name.to_ascii_uppercase())
                    .or_insert_with(|| raw_name.to_string());
            }
            let name = raw_name.to_ascii_uppercase();
            if !self.arrays.contains_key(&name) {
                return Err(self.err(ErrorCode::Undefined));
            }
            let dims = split_arguments(&spec[open + 1..close])
                .into_iter()
                .map(|arg| {
                    if arg.trim().is_empty() {
                        return Err(self.err(ErrorCode::UndefinedIndex));
                    }
                    let n = self.eval_number(&arg)?;
                    if n < 0.0 {
                        return Err(self.err(ErrorCode::InvalidValue));
                    }
                    Ok(n as usize)
                })
                .collect::<BasicResult<Vec<_>>>()?;
            if self
                .arrays
                .get(&name)
                .is_some_and(|previous| previous.dims.len() != dims.len())
            {
                return Err(self.err(ErrorCode::InvalidDimensions));
            }
            let mut next = ArrayValue::new(&name, dims);
            if let Some(previous) = self.arrays.get(&name) {
                if previous.is_string() == next.is_string()
                    && previous.dims.len() == next.dims.len()
                {
                    for flat in 0..previous.data_len() {
                        let indexes = previous.indexes_for_flat(flat);
                        if next.flat_index(&indexes).is_ok() {
                            let value = previous.get(&indexes)?;
                            next.set(&indexes, value)?;
                        }
                    }
                }
            }
            self.arrays.insert(name, next);
        }
        Ok(())
    }

    fn execute_mat(&mut self, args: &str, cursor: &Cursor) -> BasicResult<()> {
        let trimmed = args.trim();
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("BASE") {
            let value = self.eval_number(trimmed[4..].trim())? as i32;
            if value != 0 && value != 1 {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            self.mat_base = value;
            return Ok(());
        }
        if upper.starts_with("PRINT") {
            return self.execute_mat_print(trimmed[5..].trim());
        }
        if upper.starts_with("READ") {
            return self.execute_mat_read(trimmed[4..].trim());
        }
        if upper.starts_with("INPUT") {
            return self.execute_mat_input(trimmed[5..].trim(), cursor);
        }
        if find_assignment_equal(trimmed).is_some() {
            return self.execute_mat_assignment(trimmed);
        }
        Err(self.err(ErrorCode::Syntax))
    }

    fn execute_mat_read(&mut self, args: &str) -> BasicResult<()> {
        for target in split_arguments(args) {
            let name = target.trim().to_ascii_uppercase();
            if !is_basic_identifier(&name) {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            let Some(array) = self.arrays.get(&name) else {
                return Err(self.err(ErrorCode::Undefined));
            };
            if array.dims.len() > 2 {
                return Err(self.err(ErrorCode::InvalidDimensions));
            }
            let positions = self.mat_positions(&name, MatOrientation::Normal)?;
            for indexes in positions {
                if self.data_pointer >= self.data.len() {
                    return Err(self.err(ErrorCode::DataExhausted));
                }
                let value = self.data[self.data_pointer].clone();
                self.data_pointer += 1;
                let array = self.arrays.get_mut(&name).unwrap();
                if array.is_string() != matches!(value, Value::Str(_)) {
                    return Err(self.err(ErrorCode::TypeMismatch));
                }
                array.set(&indexes, value).map_err(|mut e| {
                    if e.line.is_none() {
                        e.line = self.current_line;
                    }
                    e
                })?;
            }
        }
        Ok(())
    }

    fn execute_mat_input(&mut self, args: &str, cursor: &Cursor) -> BasicResult<()> {
        if args.trim().is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }

        let mut entries = Vec::new();
        for target in split_arguments(args) {
            let raw_name = target.trim();
            if raw_name.is_empty() || raw_name.contains('(') || raw_name.contains(')') {
                return Err(self.err(ErrorCode::Syntax));
            }
            if !is_basic_identifier(raw_name) {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            let name = raw_name.to_ascii_uppercase();
            let Some(array) = self.arrays.get(&name) else {
                return Err(self.err(ErrorCode::Undefined));
            };
            if array.dims.len() > 2 {
                return Err(self.err(ErrorCode::InvalidDimensions));
            }
            entries.push(MatInputEntry {
                positions: self.mat_positions(&name, MatOrientation::Normal)?,
                name,
                position: 0,
            });
        }

        let mut queue: VecDeque<String> = VecDeque::new();
        let mut entry_idx = 0usize;
        while entry_idx < entries.len() {
            if entries[entry_idx].position >= entries[entry_idx].positions.len() {
                entry_idx += 1;
                continue;
            }

            let name = entries[entry_idx].name.clone();
            let position = entries[entry_idx].position;
            let indexes = entries[entry_idx].positions[position].clone();
            if queue.is_empty() {
                self.write(&self.mat_input_prompt(&name, &indexes));
                print!("{}", self.take_output());
                let _ = io::stdout().flush();

                let mut line = String::new();
                let bytes = {
                    let _runtime_raw_suspend = console::suspend_runtime_raw_mode().ok();
                    match io::stdin().read_line(&mut line) {
                        Ok(bytes) => bytes,
                        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                            return self.check_user_interrupt(cursor);
                        }
                        Err(err) => {
                            return Err(self
                                .err(ErrorCode::InvalidValue)
                                .with_detail(err.to_string()));
                        }
                    }
                };
                self.check_user_interrupt(cursor)?;
                if bytes == 0 {
                    println!();
                }
                let line = line.trim_end_matches(&['\r', '\n'][..]);
                let (tokens, unbalanced_quotes) = tokenize_mat_input_line(line);
                if unbalanced_quotes {
                    return Err(self.err(ErrorCode::MissingQuotes));
                }
                queue = if tokens.is_empty() {
                    VecDeque::from([String::new()])
                } else {
                    VecDeque::from(tokens)
                };
            }

            let raw_token = queue.pop_front().unwrap_or_default();
            match self.assign_mat_input_value(&name, &indexes, &raw_token) {
                Ok(()) => entries[entry_idx].position += 1,
                Err(err) => {
                    if self.error_resume_next || self.error_handler_line.is_some() {
                        self.fill_mat_input_defaults(&entries, entry_idx, position);
                        return Err(err);
                    }
                    self.write_line(&err.display_for_basic());
                    queue.clear();
                }
            }
        }
        Ok(())
    }

    fn mat_input_prompt(&self, name: &str, indexes: &[i32]) -> String {
        let index_text = indexes
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let raw = if index_text.is_empty() {
            name.to_string()
        } else {
            format!("{name}({index_text})")
        };
        format!("{}? ", apply_identifier_case(&raw, &self.identifier_case))
    }

    fn assign_mat_input_value(
        &mut self,
        name: &str,
        indexes: &[i32],
        raw_token: &str,
    ) -> BasicResult<()> {
        let raw = raw_token.trim();
        let is_string = self
            .arrays
            .get(name)
            .ok_or_else(|| self.err(ErrorCode::Undefined))?
            .is_string();
        let value = if is_string {
            if raw.is_empty() {
                Value::string("")
            } else if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
                Value::string(raw[1..raw.len() - 1].to_string())
            } else {
                return Err(self.err(ErrorCode::MissingQuotes));
            }
        } else if raw.is_empty() {
            Value::number(0.0)
        } else {
            match self.eval_value(raw)? {
                Value::Number(n) => Value::number(n),
                Value::Str(_) | Value::ArrayRef(_) => return Err(self.err(ErrorCode::TypeMismatch)),
            }
        };

        let Some(array) = self.arrays.get_mut(name) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        array.set(indexes, value).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })
    }

    fn fill_mat_input_defaults(
        &mut self,
        entries: &[MatInputEntry],
        start_entry: usize,
        start_position: usize,
    ) {
        for (idx, entry) in entries.iter().enumerate().skip(start_entry) {
            let Some(array) = self.arrays.get(&entry.name) else {
                continue;
            };
            let default = if array.is_string() {
                Value::string("")
            } else {
                Value::number(0.0)
            };
            let position = if idx == start_entry {
                start_position
            } else {
                0
            };
            let name = entry.name.clone();
            for indexes in entry.positions.iter().skip(position) {
                if let Some(array) = self.arrays.get_mut(&name) {
                    let _ = array.set(indexes, default.clone());
                }
            }
        }
    }

    fn execute_mat_print(&mut self, args: &str) -> BasicResult<()> {
        let mut body = args.trim();
        let mut using_format: Option<String> = None;
        if body.to_ascii_uppercase().starts_with("USING") {
            let Some((fmt_expr, tail)) = split_first_top_level(body[5..].trim(), ';') else {
                return Err(self.err(ErrorCode::Syntax));
            };
            using_format = Some(self.eval_value(fmt_expr.trim())?.into_string()?);
            body = tail.trim();
        }

        let items = split_print_items(body);
        let mut printed_any = false;
        for (raw_item, sep) in items {
            let item = raw_item.trim();
            if item.is_empty() {
                continue;
            }
            if printed_any {
                self.write_line("");
            }
            let (orientation, name) =
                parse_mat_print_item(item).map_err(|e| self.with_current_line(e))?;
            let wide = sep == Some(',');
            self.write_mat_array(&name, orientation, using_format.as_deref(), wide)?;
            printed_any = true;
        }
        Ok(())
    }

    fn write_mat_array(
        &mut self,
        name: &str,
        orientation: MatOrientation,
        using_format: Option<&str>,
        wide: bool,
    ) -> BasicResult<()> {
        let name = name.to_ascii_uppercase();
        let Some(array) = self.arrays.get(&name) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        if array.dims.len() > 2 {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        let (rows, cols) = self.mat_shape_for_print(&name, orientation)?;
        for r in 0..rows {
            let mut cells = Vec::with_capacity(cols);
            for c in 0..cols {
                let indexes = self.mat_indexes_for_print(&name, orientation, r, c)?;
                let value = self.arrays.get(&name).unwrap().get(&indexes)?;
                let cell = match value {
                    Value::Number(n) => {
                        if let Some(fmt) = using_format {
                            format_using_simple(n, fmt)
                        } else if wide {
                            format!("{:>22}", format_basic_number(n))
                        } else {
                            format_basic_number(n)
                        }
                    }
                    Value::Str(s) => {
                        if wide {
                            format!("{s:>22}")
                        } else {
                            s
                        }
                    }
                    Value::ArrayRef(_) => return Err(self.err(ErrorCode::TypeMismatch)),
                };
                cells.push(cell);
            }
            self.write_line(&cells.join(if wide { "" } else { "  " }));
        }
        Ok(())
    }

    fn mat_positions(&self, name: &str, orientation: MatOrientation) -> BasicResult<Vec<Vec<i32>>> {
        let name = name.to_ascii_uppercase();
        let (rows, cols) = self.mat_shape_for_print(&name, orientation)?;
        let mut positions = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                positions.push(self.mat_indexes_for_print(&name, orientation, r, c)?);
            }
        }
        Ok(positions)
    }

    fn mat_shape_for_print(
        &self,
        name: &str,
        orientation: MatOrientation,
    ) -> BasicResult<(usize, usize)> {
        let Some(array) = self.arrays.get(name) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        let lower = self.mat_base.max(0) as usize;
        let count = |bound: usize| {
            if bound < lower {
                0
            } else {
                bound - lower + 1
            }
        };
        match array.dims.as_slice() {
            [n] => {
                let len = count(*n);
                if orientation == MatOrientation::Col {
                    Ok((1, len))
                } else {
                    Ok((len, 1))
                }
            }
            [r, c] => {
                let rows = count(*r);
                let cols = count(*c);
                if orientation == MatOrientation::Col {
                    Ok((cols, rows))
                } else {
                    Ok((rows, cols))
                }
            }
            _ => Err(self.err(ErrorCode::InvalidDimensions)),
        }
    }

    fn mat_indexes_for_print(
        &self,
        name: &str,
        orientation: MatOrientation,
        row: usize,
        col: usize,
    ) -> BasicResult<Vec<i32>> {
        let Some(array) = self.arrays.get(name) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        let lower = self.mat_base.max(0) as usize;
        match array.dims.as_slice() {
            [_] => {
                let idx = if orientation == MatOrientation::Col {
                    col
                } else {
                    row
                };
                Ok(vec![(lower + idx) as i32])
            }
            [_, _] if orientation == MatOrientation::Col => {
                Ok(vec![(lower + col) as i32, (lower + row) as i32])
            }
            [_, _] => Ok(vec![(lower + row) as i32, (lower + col) as i32]),
            _ => Err(self.err(ErrorCode::InvalidDimensions)),
        }
    }

    fn execute_mat_assignment(&mut self, statement: &str) -> BasicResult<()> {
        let Some(pos) = find_assignment_equal(statement) else {
            return Err(self.err(ErrorCode::Syntax));
        };
        let target = statement[..pos].trim().to_ascii_uppercase();
        let rhs = statement[pos + 1..].trim();
        if !is_basic_identifier(&target) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        if !self.arrays.contains_key(&target) {
            self.arrays
                .insert(target.clone(), ArrayValue::new(&target, vec![10]));
        }
        let rhs_upper = rhs.to_ascii_uppercase();
        if rhs_upper == "CON" {
            return self.mat_fill_array(&target, Value::number(1.0));
        }
        if rhs_upper == "ZER" {
            return self.mat_fill_array(&target, Value::number(0.0));
        }
        if rhs_upper == "IDN" {
            return self.mat_identity(&target);
        }

        match self.eval_mat_expr(rhs)? {
            MatExprValue::Scalar(value) => {
                self.mat_fill_array(&target, value)?;
                self.return_array_for_active_function(&target);
                Ok(())
            }
            MatExprValue::Matrix(matrix) => {
                let returning_from_active_function = self
                    .active_function_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&target));
                if matrix.dims.len() > 2 && !returning_from_active_function {
                    return Err(self.err(ErrorCode::InvalidDimensions));
                }
                if matrix.is_string()
                    != self
                        .arrays
                        .get(&target)
                        .map(|array| array.is_string())
                        .unwrap_or_else(|| target.ends_with('$'))
                {
                    return Err(self.err(ErrorCode::TypeMismatch));
                }
                self.arrays.insert(target.clone(), matrix);
                self.return_array_for_active_function(&target);
                Ok(())
            }
        }
    }

    fn mat_fill_array(&mut self, target: &str, value: Value) -> BasicResult<()> {
        let Some(array) = self.arrays.get_mut(target) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        array.fill(value).map_err(|mut e| {
            if e.line.is_none() {
                e.line = self.current_line;
            }
            e
        })
    }

    fn mat_identity(&mut self, target: &str) -> BasicResult<()> {
        let Some(array) = self.arrays.get(target) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        if array.is_string() {
            return Err(self.err(ErrorCode::TypeMismatch));
        }
        if array.dims.len() != 2 || array.dims[0] != array.dims[1] {
            return Err(self.err(ErrorCode::MatIdnDimension));
        }
        let dim = array.dims[0];
        let target = target.to_string();
        self.mat_fill_array(&target, Value::number(0.0))?;
        let start = self.mat_base.max(0) as usize;
        for i in start..=dim {
            let array = self.arrays.get_mut(&target).unwrap();
            array
                .set(&[i as i32, i as i32], Value::number(1.0))
                .map_err(|mut e| {
                    if e.line.is_none() {
                        e.line = self.current_line;
                    }
                    e
                })?;
        }
        Ok(())
    }

    fn eval_mat_expr(&mut self, source: &str) -> BasicResult<MatExprValue> {
        let original = source.trim();
        let wrapped_inner = if original.starts_with('(') && original.ends_with(')') {
            Some(strip_wrapping_parens(original))
        } else {
            None
        };
        if wrapped_inner.is_some_and(|inner| scalar_times_matrix_div_scalar(inner, &self.arrays))
            || mat_expr_has_array_before_function(original, &self.arrays)
        {
            return Err(self.err(ErrorCode::ForbiddenExpression));
        }
        let expr = strip_wrapping_parens(source).trim();
        if expr.is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }

        if let Some((pos, op)) = find_top_level_mat_operator(expr, &['+', '-']) {
            let left = self.eval_mat_expr(&expr[..pos])?;
            let right = self.eval_mat_expr(&expr[pos + op.len_utf8()..])?;
            return self.mat_binary(left, right, op);
        }
        if let Some((pos, op)) = find_top_level_mat_operator(expr, &['*', '/']) {
            let left = self.eval_mat_expr(&expr[..pos])?;
            let right = self.eval_mat_expr(&expr[pos + op.len_utf8()..])?;
            return self.mat_binary(left, right, op);
        }
        if let Some((pos, op)) = find_top_level_mat_operator(expr, &['^']) {
            let left = self.eval_mat_expr(&expr[..pos])?;
            let right = self.eval_mat_expr(&expr[pos + op.len_utf8()..])?;
            return self.mat_binary(left, right, op);
        }
        if let Some(rest) = expr.strip_prefix('-') {
            let value = self.eval_mat_expr(rest)?;
            return self.mat_unary_minus(value);
        }

        let upper = expr.to_ascii_uppercase();
        if let Some(inner) = whole_function_argument(expr, "TRN") {
            let value = self.eval_mat_expr(inner)?;
            let matrix = self.mat_value_to_numeric_matrix(value)?;
            let mut out = vec![0.0; matrix.rows * matrix.cols];
            for r in 0..matrix.rows {
                for c in 0..matrix.cols {
                    out[c * matrix.rows + r] = matrix.data[r * matrix.cols + c];
                }
            }
            return Ok(MatExprValue::Matrix(ArrayValue::from_numeric_matrix(
                "",
                self.mat_base,
                matrix.cols,
                matrix.rows,
                out,
            )));
        }
        if let Some(inner) = whole_function_argument(expr, "INV") {
            let value = self.eval_mat_expr(inner)?;
            let matrix = self.mat_value_to_numeric_matrix(value)?;
            let (inverse, _) = self.invert_numeric_matrix(matrix)?;
            return Ok(MatExprValue::Matrix(inverse));
        }
        if looks_like_non_fn_function_call(expr) && mat_expr_mentions_array(expr, &self.arrays) {
            return Err(self.err(ErrorCode::ForbiddenExpression));
        }

        if is_basic_identifier(&upper) {
            if let Some(array) = self.arrays.get(&upper) {
                return Ok(MatExprValue::Matrix(array.clone()));
            }
        }
        match self.eval_value(expr)? {
            Value::ArrayRef(name) => {
                let Some(array) = self.arrays.get(&name).cloned() else {
                    return Err(self.err(ErrorCode::Undefined));
                };
                Ok(MatExprValue::Matrix(array))
            }
            value => Ok(MatExprValue::Scalar(value)),
        }
    }

    fn mat_unary_minus(&mut self, value: MatExprValue) -> BasicResult<MatExprValue> {
        match value {
            MatExprValue::Scalar(value) => {
                Ok(MatExprValue::Scalar(Value::number(-value.as_number()?)))
            }
            MatExprValue::Matrix(array) => {
                let matrix = self.array_to_numeric_matrix(&array)?;
                Ok(MatExprValue::Matrix(ArrayValue::from_numeric_matrix(
                    "",
                    self.mat_base,
                    matrix.rows,
                    matrix.cols,
                    matrix.data.into_iter().map(|v| -v).collect(),
                )))
            }
        }
    }

    fn mat_binary(
        &mut self,
        left: MatExprValue,
        right: MatExprValue,
        op: char,
    ) -> BasicResult<MatExprValue> {
        match (left, right) {
            (MatExprValue::Scalar(l), MatExprValue::Scalar(r)) => {
                let value = match op {
                    '+' => match (l, r) {
                        (Value::Str(a), Value::Str(b)) => Value::string(format!("{a}{b}")),
                        (Value::Number(a), Value::Number(b)) => Value::number(a + b),
                        _ => return Err(self.err(ErrorCode::TypeMismatch)),
                    },
                    '-' => Value::number(l.as_number()? - r.as_number()?),
                    '*' => Value::number(l.as_number()? * r.as_number()?),
                    '/' => {
                        let divisor = r.as_number()?;
                        if divisor == 0.0 {
                            return Err(self.err(ErrorCode::DivisionByZero));
                        }
                        Value::number(l.as_number()? / divisor)
                    }
                    '^' => Value::number(l.as_number()?.powf(r.as_number()?)),
                    _ => return Err(self.err(ErrorCode::Syntax)),
                };
                Ok(MatExprValue::Scalar(value))
            }
            (MatExprValue::Matrix(matrix), MatExprValue::Scalar(scalar)) => {
                if matrix.is_string() {
                    return Err(self.err(ErrorCode::ForbiddenExpression));
                }
                self.mat_matrix_scalar(matrix, scalar.as_number()?, op, false)
            }
            (MatExprValue::Scalar(scalar), MatExprValue::Matrix(matrix)) => {
                if matrix.is_string() {
                    return Err(self.err(ErrorCode::ForbiddenExpression));
                }
                self.mat_matrix_scalar(matrix, scalar.as_number()?, op, true)
            }
            (MatExprValue::Matrix(left), MatExprValue::Matrix(right)) => match op {
                '+' if left.is_string() || right.is_string() => {
                    self.mat_string_matrix_add(left, right)
                }
                '-' if left.is_string() || right.is_string() => {
                    Err(self.err(ErrorCode::ForbiddenExpression))
                }
                '+' | '-' => self.mat_matrix_add_sub(left, right, op),
                '*' => self.mat_matrix_multiply(left, right),
                '/' => Err(self.err(ErrorCode::Undefined)),
                '^' => Err(self.err(ErrorCode::ForbiddenExpression)),
                _ => Err(self.err(ErrorCode::Syntax)),
            },
        }
    }

    fn mat_string_matrix_add(
        &mut self,
        left: ArrayValue,
        right: ArrayValue,
    ) -> BasicResult<MatExprValue> {
        if !left.is_string() || !right.is_string() {
            return Err(self.err(ErrorCode::TypeMismatch));
        }
        if left.dims != right.dims {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        let mut out = ArrayValue::new("$", left.dims.clone());
        for flat in 0..left.data_len() {
            let indexes = left.indexes_for_flat(flat);
            let a = left.get(&indexes)?.into_string()?;
            let b = right.get(&indexes)?.into_string()?;
            out.set(&indexes, Value::string(format!("{a}{b}")))?;
        }
        Ok(MatExprValue::Matrix(out))
    }

    fn mat_matrix_scalar(
        &mut self,
        matrix: ArrayValue,
        scalar: f64,
        op: char,
        scalar_left: bool,
    ) -> BasicResult<MatExprValue> {
        let matrix = self.array_to_numeric_matrix(&matrix)?;
        let data = matrix
            .data
            .into_iter()
            .map(|v| match op {
                '+' => Ok(v + scalar),
                '-' if scalar_left => Ok(scalar - v),
                '-' => Ok(v - scalar),
                '*' => Ok(v * scalar),
                '/' if scalar_left => {
                    if v == 0.0 {
                        Err(self.err(ErrorCode::DivisionByZero))
                    } else {
                        Ok(scalar / v)
                    }
                }
                '/' => {
                    if scalar == 0.0 {
                        Err(self.err(ErrorCode::DivisionByZero))
                    } else {
                        Ok(v / scalar)
                    }
                }
                '^' if scalar_left => Ok(scalar.powf(v)),
                '^' => Ok(v.powf(scalar)),
                _ => Err(self.err(ErrorCode::ForbiddenExpression)),
            })
            .collect::<BasicResult<Vec<_>>>()?;
        Ok(MatExprValue::Matrix(ArrayValue::from_numeric_matrix(
            "",
            self.mat_base,
            matrix.rows,
            matrix.cols,
            data,
        )))
    }

    fn mat_matrix_add_sub(
        &mut self,
        left: ArrayValue,
        right: ArrayValue,
        op: char,
    ) -> BasicResult<MatExprValue> {
        let left = self.array_to_numeric_matrix(&left)?;
        let right = self.array_to_numeric_matrix(&right)?;
        if left.rows != right.rows || left.cols != right.cols {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        let data = left
            .data
            .into_iter()
            .zip(right.data)
            .map(|(a, b)| if op == '+' { a + b } else { a - b })
            .collect();
        Ok(MatExprValue::Matrix(ArrayValue::from_numeric_matrix(
            "",
            self.mat_base,
            left.rows,
            left.cols,
            data,
        )))
    }

    fn mat_matrix_multiply(
        &mut self,
        left: ArrayValue,
        right: ArrayValue,
    ) -> BasicResult<MatExprValue> {
        let left = self.array_to_numeric_matrix(&left)?;
        let right = self.array_to_numeric_matrix(&right)?;
        if left.cols != right.rows {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        let mut data = vec![0.0; left.rows * right.cols];
        for r in 0..left.rows {
            for c in 0..right.cols {
                let mut sum = 0.0;
                for k in 0..left.cols {
                    sum += left.data[r * left.cols + k] * right.data[k * right.cols + c];
                }
                data[r * right.cols + c] = sum;
            }
        }
        Ok(MatExprValue::Matrix(ArrayValue::from_numeric_matrix(
            "",
            self.mat_base,
            left.rows,
            right.cols,
            data,
        )))
    }

    fn mat_value_to_numeric_matrix(&self, value: MatExprValue) -> BasicResult<NumericMatrix> {
        match value {
            MatExprValue::Matrix(array) => self.array_to_numeric_matrix(&array),
            MatExprValue::Scalar(_) => Err(self.err(ErrorCode::ForbiddenExpression)),
        }
    }

    fn array_to_numeric_matrix(&self, array: &ArrayValue) -> BasicResult<NumericMatrix> {
        if array.is_string() {
            return Err(self.err(ErrorCode::TypeMismatch));
        }
        if array.dims.len() > 2 {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        let lower = self.mat_base.max(0) as usize;
        let count = |bound: usize| {
            if bound < lower {
                0
            } else {
                bound - lower + 1
            }
        };
        let (rows, cols) = match array.dims.as_slice() {
            [n] => (count(*n), 1),
            [r, c] => (count(*r), count(*c)),
            _ => return Err(self.err(ErrorCode::InvalidDimensions)),
        };
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let indexes = if array.dims.len() == 1 {
                    vec![(lower + r) as i32]
                } else {
                    vec![(lower + r) as i32, (lower + c) as i32]
                };
                data.push(array.get(&indexes)?.as_number()?);
            }
        }
        Ok(NumericMatrix { rows, cols, data })
    }

    fn invert_numeric_matrix(&self, matrix: NumericMatrix) -> BasicResult<(ArrayValue, f64)> {
        if matrix.rows != matrix.cols {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        let n = matrix.rows;
        let mut a = matrix.data;
        let mut inv = vec![0.0; n * n];
        for i in 0..n {
            inv[i * n + i] = 1.0;
        }
        let mut det = 1.0;
        let mut sign = 1.0;
        for col in 0..n {
            let mut pivot = col;
            let mut pivot_abs = a[col * n + col].abs();
            for row in col + 1..n {
                let value = a[row * n + col].abs();
                if value > pivot_abs {
                    pivot = row;
                    pivot_abs = value;
                }
            }
            if pivot_abs == 0.0 {
                return Err(self.err(ErrorCode::DivisionByZero));
            }
            if pivot != col {
                for c in 0..n {
                    a.swap(col * n + c, pivot * n + c);
                    inv.swap(col * n + c, pivot * n + c);
                }
                sign = -sign;
            }
            let pivot_value = a[col * n + col];
            let pivot_inv = 1.0 / pivot_value;
            det *= pivot_value;
            for c in 0..n {
                a[col * n + c] *= pivot_inv;
                inv[col * n + c] *= pivot_inv;
            }
            for row in 0..n {
                if row == col {
                    continue;
                }
                let factor = a[row * n + col];
                if factor == 0.0 {
                    continue;
                }
                for c in 0..n {
                    a[row * n + c] -= factor * a[col * n + c];
                    inv[row * n + c] -= factor * inv[col * n + c];
                }
            }
        }
        Ok((
            ArrayValue::from_numeric_matrix("", self.mat_base, n, n, inv),
            det * sign,
        ))
    }

    fn determinant_numeric_matrix(&self, matrix: NumericMatrix) -> BasicResult<f64> {
        if matrix.rows != matrix.cols {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        let n = matrix.rows;
        let mut a = matrix.data;
        let mut det = 1.0;
        for col in 0..n {
            let mut pivot = col;
            let mut pivot_abs = a[col * n + col].abs();
            for row in col + 1..n {
                let value = a[row * n + col].abs();
                if value > pivot_abs {
                    pivot = row;
                    pivot_abs = value;
                }
            }
            if pivot_abs <= 1e-12 {
                return Ok(0.0);
            }
            if pivot != col {
                for c in 0..n {
                    a.swap(col * n + c, pivot * n + c);
                }
                det = -det;
            }
            let pivot_value = a[col * n + col];
            det *= pivot_value;
            for row in col + 1..n {
                let factor = a[row * n + col] / pivot_value;
                for c in col + 1..n {
                    a[row * n + c] -= factor * a[col * n + c];
                }
                a[row * n + col] = 0.0;
            }
        }
        Ok(det)
    }

    fn execute_read(&mut self, args: &str) -> BasicResult<()> {
        for target in split_arguments(args) {
            if self.data_pointer >= self.data.len() {
                return Err(self.err(ErrorCode::DataExhausted));
            }
            let value = self.data[self.data_pointer].clone();
            self.data_pointer += 1;
            self.assign(&target, value)?;
        }
        Ok(())
    }

    fn execute_randomize(&mut self, arg: &str) -> BasicResult<()> {
        if arg.trim().is_empty() {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            self.rng = SimpleRng::new(seed);
        } else {
            let seed = self.eval_number(arg)? as u64;
            self.rng = SimpleRng::new(seed);
        }
        Ok(())
    }

    fn execute_pause(&mut self, arg: &str, cursor: &Cursor) -> BasicResult<()> {
        if arg.trim().is_empty() {
            if self.graphics_window.is_some()
                && self.graphics_window_enabled
                && self.current_run_uses_graphics_window()
            {
                loop {
                    std::thread::sleep(Duration::from_millis(2));
                    self.check_user_interrupt(cursor)?;
                    self.process_timers()?;
                    self.check_user_interrupt(cursor)?;
                    if self.mouse_state.event == "LEFTDOWN" && !self.mouse_event_consumed {
                        self.graphics
                            .set_cursor_from_logical_screen(self.mouse_state.x, self.mouse_state.y);
                        break;
                    }
                    if self.end_requested
                        || self.stopped_cursor.is_some()
                        || self.function_return_requested
                        || self.sub_return_requested
                    {
                        break;
                    }
                }
                return Ok(());
            }
            loop {
                std::thread::sleep(Duration::from_millis(20));
                self.check_user_interrupt(cursor)?;
                self.process_timers()?;
                self.check_user_interrupt(cursor)?;
                if self.end_requested
                    || self.stopped_cursor.is_some()
                    || self.function_return_requested
                    || self.sub_return_requested
                {
                    break;
                }
                if !self
                    .timers
                    .iter()
                    .any(|timer| timer.active || timer.pending)
                    && self.mouse_handlers.is_empty()
                {
                    break;
                }
            }
            return Ok(());
        }

        let ms = self.eval_number(arg)?.max(0.0) as u64;
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            let now = Instant::now();
            let remaining = deadline.saturating_duration_since(now);
            std::thread::sleep(remaining.min(Duration::from_millis(20)));
            self.check_user_interrupt(cursor)?;
            self.process_timers()?;
            self.check_user_interrupt(cursor)?;
            if self.end_requested
                || self.stopped_cursor.is_some()
                || self.function_return_requested
                || self.sub_return_requested
            {
                break;
            }
        }
        Ok(())
    }

    fn execute_timer(&mut self, args: &str, repeat: bool) -> BasicResult<()> {
        let upper = args.to_ascii_uppercase();
        let Some(pos) = upper.find(" GOSUB ") else {
            return Err(self.err(ErrorCode::Syntax));
        };
        let before = args[..pos].trim();
        let target = parse_line_number_literal(&args[pos + 7..])
            .ok_or_else(|| self.err(ErrorCode::InvalidLineNumber))?;
        let parts = split_arguments(before);
        if parts.is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }
        let ticks = self.eval_number(&parts[0])?.max(0.0) as i32;
        let interval = Duration::from_millis((ticks as u64).saturating_mul(20));
        self.timers.push(BasicTimer {
            interval,
            next_fire: Instant::now() + interval,
            target,
            repeat,
            active: true,
            pending: false,
            pending_delay: 0,
            remaining_ticks: ticks,
        });
        Ok(())
    }

    fn process_timers(&mut self) -> BasicResult<()> {
        if self.timers.is_empty() {
            return Ok(());
        }
        let now = Instant::now();
        for timer in &mut self.timers {
            if timer.active && now >= timer.next_fire {
                timer.pending = true;
            }
        }
        for timer in &mut self.timers {
            if timer.pending && timer.pending_delay > 0 {
                timer.pending_delay -= 1;
            }
        }
        if !self.interrupts_enabled {
            return Ok(());
        }
        let Some(index) = self
            .timers
            .iter()
            .position(|timer| timer.active && timer.pending && timer.pending_delay == 0)
        else {
            return Ok(());
        };
        let target = self.timers[index].target;
        if self.timers[index].repeat {
            self.timers[index].pending = false;
            let mut next = self.timers[index].next_fire + self.timers[index].interval;
            while next <= now {
                next += self.timers[index].interval;
            }
            self.timers[index].next_fire = next;
        } else {
            self.timers[index].pending = false;
            self.timers[index].active = false;
            self.timers[index].remaining_ticks = 0;
        }
        self.execute_inline_gosub(&target.to_string())
    }

    fn remain_value(&mut self) -> f64 {
        let Some(timer) = self.timers.iter_mut().find(|timer| timer.active) else {
            return 0.0;
        };
        let value = timer.remaining_ticks.max(0);
        if value > 0 {
            timer.remaining_ticks -= 1;
            if timer.remaining_ticks == 0 {
                timer.pending = true;
                timer.pending_delay = 2;
            }
        }
        value as f64
    }

    fn execute_def(&mut self, command: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let text = command.trim();
        let upper = text.to_ascii_uppercase();
        if upper.starts_with("DEF SUB") {
            return self.execute_def_sub(text, cursor);
        }
        if !upper.starts_with("DEF FN") {
            return Err(self.err(ErrorCode::Syntax));
        }
        let rest = &text[4..];
        let (header, expr) = if let Some(eq) = rest.find('=') {
            (rest[..eq].trim(), Some(rest[eq + 1..].trim().to_string()))
        } else {
            (rest.trim(), None)
        };
        let (name, params) = parse_function_header(header)?;
        if let Some(expr) = expr {
            self.detach_multiline_function(&name);
            self.functions
                .insert(name, UserFunction::Single { params, expr });
            return Ok(());
        }
        let end = self.find_matching_routine_end(cursor, "FNEND", ErrorCode::FnEndWithoutDef)?;
        let start = Cursor {
            line_idx: cursor.line_idx + 1,
            cmd_idx: 0,
        };
        let local_specs =
            self.collect_leading_local_specs(start.line_idx, end.line_idx, "FNEND")?;
        self.validate_local_specs(&name, &params, &local_specs)?;
        self.detach_multiline_function(&name);
        for idx in start.line_idx..=end.line_idx {
            if let Some(line) = self.program.line_numbers().get(idx).copied() {
                self.fn_line_owner.insert(line, name.clone());
            }
        }
        self.functions.insert(
            name,
            UserFunction::Multi {
                params,
                local_specs,
                start,
                end: end.clone(),
            },
        );
        *cursor = self.cursor_after_cached_command(Cursor {
            line_idx: end.line_idx,
            cmd_idx: end.cmd_idx + 1,
        });
        Ok(())
    }

    fn execute_def_sub(&mut self, command: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let header = command[7..].trim();
        let (name, params) = parse_sub_header(header)?;
        let end = self.find_matching_routine_end(cursor, "SUBEND", ErrorCode::SubEndWithoutDef)?;
        let start = Cursor {
            line_idx: cursor.line_idx + 1,
            cmd_idx: 0,
        };
        let local_specs =
            self.collect_leading_local_specs(start.line_idx, end.line_idx, "SUBEND")?;
        self.validate_local_specs(&name, &params, &local_specs)?;
        self.detach_multiline_sub(&name);
        for idx in start.line_idx..=end.line_idx {
            if let Some(line) = self.program.line_numbers().get(idx).copied() {
                self.sub_line_owner.insert(line, name.clone());
            }
        }
        self.subs.insert(
            name,
            UserSub {
                params,
                local_specs,
                start,
                end: end.clone(),
            },
        );
        *cursor = self.cursor_after_cached_command(Cursor {
            line_idx: end.line_idx,
            cmd_idx: end.cmd_idx + 1,
        });
        Ok(())
    }

    fn execute_fnend(&mut self) -> BasicResult<()> {
        if self.inside_multiline_function() {
            self.function_return_requested = true;
            Ok(())
        } else {
            Err(self.err(ErrorCode::FnEndWithoutDef))
        }
    }

    fn execute_fnexit(&mut self) -> BasicResult<()> {
        self.execute_fnend()
    }

    fn execute_subend(&mut self) -> BasicResult<()> {
        if self.inside_multiline_sub() {
            self.sub_return_requested = true;
            Ok(())
        } else {
            Err(self.err(ErrorCode::SubEndWithoutDef))
        }
    }

    fn execute_subexit(&mut self) -> BasicResult<()> {
        self.execute_subend()
    }

    fn execute_local(&mut self) -> BasicResult<()> {
        if self.inside_multiline_routine() {
            Ok(())
        } else {
            Err(self.err(ErrorCode::Syntax))
        }
    }

    fn find_matching_routine_end(
        &self,
        cursor: &Cursor,
        terminator: &str,
        error: ErrorCode,
    ) -> BasicResult<Cursor> {
        let lines = self.program.line_numbers();
        for line_idx in cursor.line_idx + 1..lines.len() {
            let line_no = lines[line_idx];
            let commands = if let Some(cached) = self.command_cache.get(&line_no) {
                cached.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            } else {
                split_commands(self.program.get(line_no).unwrap_or(""))
            };
            if commands.len() == 1 && commands[0].trim().eq_ignore_ascii_case(terminator) {
                return Ok(Cursor {
                    line_idx,
                    cmd_idx: 0,
                });
            }
        }
        Err(self.err(error))
    }

    fn collect_leading_local_specs(
        &self,
        start_line_idx: usize,
        end_line_idx: usize,
        terminator: &str,
    ) -> BasicResult<Vec<LocalSpec>> {
        let mut local_specs = Vec::new();
        let mut seen_nonlocal = false;
        let mut seen_names: Vec<String> = Vec::new();
        let lines = self.program.line_numbers();

        for line_idx in start_line_idx..=end_line_idx {
            let Some(line_no) = lines.get(line_idx).copied() else {
                break;
            };
            let commands = if let Some(cached) = self.command_cache.get(&line_no) {
                cached.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            } else {
                split_commands(self.program.get(line_no).unwrap_or(""))
            };
            for command in commands {
                let trimmed = command.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.eq_ignore_ascii_case(terminator) {
                    return Ok(local_specs);
                }
                if first_word_is(trimmed, "REM") {
                    continue;
                }
                if first_word_is(trimmed, "LOCAL") {
                    if seen_nonlocal {
                        return Err(BasicError::new(ErrorCode::LocalNotAtStart).at_line(line_no));
                    }
                    let parsed = parse_local_specs(trimmed[5..].trim()).map_err(|mut e| {
                        if e.line.is_none() {
                            e.line = Some(line_no);
                        }
                        e
                    })?;
                    for spec in parsed {
                        let name = local_spec_name(&spec).to_string();
                        if seen_names.iter().any(|existing| existing == &name) {
                            return Err(
                                BasicError::new(ErrorCode::InvalidArgument).at_line(line_no)
                            );
                        }
                        seen_names.push(name);
                        local_specs.push(spec);
                    }
                    continue;
                }
                seen_nonlocal = true;
            }
        }
        Ok(local_specs)
    }

    fn validate_local_specs(
        &self,
        routine_name: &str,
        params: &[String],
        local_specs: &[LocalSpec],
    ) -> BasicResult<()> {
        for spec in local_specs {
            let name = local_spec_name(spec);
            if name.eq_ignore_ascii_case(routine_name)
                || params.iter().any(|param| param.eq_ignore_ascii_case(name))
            {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
        }
        Ok(())
    }

    fn cursor_after_cached_command(&self, mut cursor: Cursor) -> Cursor {
        let Some(line) = self.program.line_numbers().get(cursor.line_idx).copied() else {
            return cursor;
        };
        let len = self.command_cache.get(&line).map_or_else(
            || split_commands(self.program.get(line).unwrap_or("")).len(),
            Vec::len,
        );
        if cursor.cmd_idx >= len {
            cursor.line_idx += 1;
            cursor.cmd_idx = 0;
        }
        cursor
    }

    fn detach_multiline_function(&mut self, name: &str) {
        let Some(UserFunction::Multi { start, end, .. }) = self.functions.get(name).cloned() else {
            return;
        };
        for idx in start.line_idx..=end.line_idx {
            if let Some(line) = self.program.line_numbers().get(idx).copied() {
                self.fn_line_owner.remove(&line);
            }
        }
    }

    fn detach_multiline_sub(&mut self, name: &str) {
        let Some(UserSub { start, end, .. }) = self.subs.get(name).cloned() else {
            return;
        };
        for idx in start.line_idx..=end.line_idx {
            if let Some(line) = self.program.line_numbers().get(idx).copied() {
                self.sub_line_owner.remove(&line);
            }
        }
    }

    fn inside_multiline_function(&self) -> bool {
        !self.handling_error && !self.active_functions.is_empty()
    }

    fn inside_multiline_sub(&self) -> bool {
        !self.handling_error && !self.active_subs.is_empty()
    }

    fn inside_multiline_routine(&self) -> bool {
        self.inside_multiline_function() || self.inside_multiline_sub()
    }

    fn active_function_name(&self) -> Option<&str> {
        self.active_functions
            .last()
            .map(|frame| frame.name.as_str())
    }

    fn active_sub_name(&self) -> Option<&str> {
        self.active_subs.last().map(|frame| frame.name.as_str())
    }

    fn function_for_line(&self, line: i32) -> Option<&str> {
        self.fn_line_owner.get(&line).map(String::as_str)
    }

    fn sub_for_line(&self, line: i32) -> Option<&str> {
        self.sub_line_owner.get(&line).map(String::as_str)
    }

    fn return_value_for_active_function(&mut self, lhs: &str, value: &Value) {
        let Some(frame) = self.active_functions.last_mut() else {
            return;
        };
        if lhs.trim().eq_ignore_ascii_case(&frame.name) {
            frame.return_value = Some(value.clone());
        }
    }

    fn return_array_for_active_function(&mut self, name: &str) {
        let Some(frame) = self.active_functions.last_mut() else {
            return;
        };
        if name.trim().eq_ignore_ascii_case(&frame.name) {
            frame.return_value = Some(Value::ArrayRef(name.to_ascii_uppercase()));
        }
    }

    fn active_function_variable(&self, name: &str) -> Option<Value> {
        let frame = self.active_functions.last()?;
        if !frame.name.eq_ignore_ascii_case(name) {
            return None;
        }
        if name.ends_with('$') {
            Some(Value::string(
                self.string_variables.get(name).cloned().unwrap_or_default(),
            ))
        } else {
            Some(Value::number(
                self.numeric_variables.get(name).copied().unwrap_or(0.0),
            ))
        }
    }

    fn ensure_graphics_window(&mut self) -> BasicResult<()> {
        if !self.graphics_window_enabled || self.graphics_window_suppressed {
            return Ok(());
        }
        self.prepare_graphics_window_use_by_current_run()?;
        self.pump_graphics_window_if_due()?;
        let recreate = self
            .graphics_window
            .as_ref()
            .is_none_or(|window| !window.matches_size(&self.graphics));
        if recreate {
            self.graphics_window = Some(GraphicsWindow::new(&self.graphics)?);
            self.graphics_window_dirty = false;
            self.last_graphics_window_pump = Instant::now();
            self.last_graphics_window_present = Instant::now() - Duration::from_millis(16);
            refocus_console_window();
            self.process_mouse_event()?;
        }
        Ok(())
    }

    fn refresh_graphics_window(&mut self) -> BasicResult<()> {
        if !self.graphics_window_enabled || self.graphics_window_suppressed {
            return Ok(());
        }
        self.graphics_window_dirty = true;
        if self.current_line.is_none() {
            self.present_graphics_window()
        } else {
            self.ensure_graphics_window()?;
            self.pump_graphics_window_if_due()
        }
    }

    fn present_graphics_window(&mut self) -> BasicResult<()> {
        if !self.graphics_window_enabled || self.graphics_window_suppressed {
            return Ok(());
        }
        self.prepare_graphics_window_use_by_current_run()?;
        let recreate = self
            .graphics_window
            .as_ref()
            .is_none_or(|window| !window.matches_size(&self.graphics));
        if recreate {
            self.graphics_window = Some(GraphicsWindow::new(&self.graphics)?);
            self.graphics_window_dirty = false;
            self.last_graphics_window_pump = Instant::now();
            self.last_graphics_window_present = Instant::now() - Duration::from_millis(16);
            refocus_console_window();
            return Ok(());
        }
        let mut user_closed = false;
        if let Some(window) = self.graphics_window.as_mut() {
            if !window.is_open() {
                user_closed = true;
            } else {
                self.mouse_state = window.present(&self.graphics)?;
                self.mouse_event_consumed = false;
                user_closed = !window.is_open();
            }
        }
        if user_closed {
            self.mark_graphics_window_user_closed();
        } else if self.graphics_window.is_some() {
            self.graphics_window_dirty = false;
            self.last_graphics_window_pump = Instant::now();
            self.last_graphics_window_present = Instant::now();
            self.process_mouse_event()?;
            if self.current_line.is_none() {
                refocus_console_window();
            }
        }
        Ok(())
    }

    fn execute_frame(&mut self, arg: &str, cursor: &Cursor) -> BasicResult<()> {
        let arg = arg.trim();
        if arg.is_empty() {
            self.graphics_window_dirty = true;
            self.record_frame_command_present_at(Instant::now());
            self.present_graphics_window()?;
            return Ok(());
        }

        let parts = split_arguments(arg);
        let frame_time = match parts.len() {
            1 => self
                .wait_for_frame_rate(&parts[0], cursor)?
                .unwrap_or_else(Instant::now),
            _ => return Err(self.err(ErrorCode::ArgumentMismatch)),
        };

        self.graphics_window_dirty = true;
        self.record_frame_command_present_at(frame_time);
        self.present_graphics_window()?;
        Ok(())
    }

    fn record_frame_command_present_at(&mut self, timestamp: Instant) {
        if self.graphics_window_enabled
            && !self.graphics_window_suppressed
            && self.graphics_window.is_some()
        {
            self.last_frame_command_present = Some(timestamp);
        }
    }

    fn wait_for_frame_rate(
        &mut self,
        fps_arg: &str,
        cursor: &Cursor,
    ) -> BasicResult<Option<Instant>> {
        let fps = self.eval_number(fps_arg)?;
        if !fps.is_finite() || fps <= 0.0 {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        if !self.graphics_window_enabled || self.graphics_window_suppressed {
            return Ok(None);
        }
        let Some(last_present) = self.last_frame_command_present else {
            return Ok(None);
        };
        let seconds_per_frame = 1.0 / fps;
        if !seconds_per_frame.is_finite() || seconds_per_frame > u64::MAX as f64 {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let frame_interval = Duration::from_secs_f64(seconds_per_frame);
        let Some(deadline) = last_present.checked_add(frame_interval) else {
            return Err(self.err(ErrorCode::InvalidArgument));
        };
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(remaining.min(Duration::from_millis(2)));
            self.check_user_interrupt(cursor)?;
            self.process_timers()?;
            self.check_user_interrupt(cursor)?;
            if self.end_requested
                || self.stopped_cursor.is_some()
                || self.function_return_requested
                || self.sub_return_requested
            {
                break;
            }
        }
        let now = Instant::now();
        if now.saturating_duration_since(deadline) > frame_interval {
            Ok(Some(now))
        } else {
            Ok(Some(deadline))
        }
    }

    fn pump_graphics_window_if_due(&mut self) -> BasicResult<()> {
        if !self.graphics_window_enabled
            || self.graphics_window_suppressed
            || self.graphics_window.is_none()
        {
            return Ok(());
        }
        if self.last_graphics_window_pump.elapsed() < Duration::from_millis(2) {
            return Ok(());
        }
        self.last_graphics_window_pump = Instant::now();
        let mut user_closed = false;
        if let Some(window) = self.graphics_window.as_mut() {
            self.mouse_state = window.pump_events();
            self.mouse_event_consumed = false;
            if !window.is_open() {
                user_closed = true;
            }
        }
        if user_closed {
            self.mark_graphics_window_user_closed();
        } else {
            self.process_mouse_event()?;
        }
        Ok(())
    }

    fn process_mouse_event(&mut self) -> BasicResult<()> {
        if self.mouse_state.event.is_empty()
            || !self.interrupts_enabled
            || self.run_depth == 0
            || self.mouse_event_consumed
            || self.handling_mouse_event
        {
            return Ok(());
        }
        let Some(target) = self.mouse_handlers.get(&self.mouse_state.event).copied() else {
            return Ok(());
        };
        self.handling_mouse_event = true;
        let result = self.execute_inline_gosub(&target.to_string());
        self.handling_mouse_event = false;
        self.mouse_event_consumed = true;
        result
    }

    fn pump_graphics_window_for_console(&mut self) -> io::Result<()> {
        self.pump_graphics_window_if_due()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.display_for_basic()))
    }

    fn prepare_graphics_window_use_by_current_run(&mut self) -> BasicResult<()> {
        if self.current_line.is_some() && !self.graphics_window_used_this_run {
            self.pump_graphics_window_if_due()?;
        }
        self.mark_graphics_window_used_by_current_run();
        Ok(())
    }

    fn mark_graphics_window_used_by_current_run(&mut self) {
        if self.current_line.is_some() {
            self.graphics_window_used_this_run = true;
        }
    }

    fn current_run_uses_graphics_window(&self) -> bool {
        self.current_line.is_some() && self.graphics_window_used_this_run
    }

    fn mark_graphics_window_user_closed(&mut self) {
        let interrupt_current_run = self.current_run_uses_graphics_window();
        self.graphics_window = None;
        self.graphics_window_suppressed = false;
        self.graphics.reset_state();
        self.graphics_window_dirty = false;
        self.last_frame_command_present = None;
        if interrupt_current_run {
            self.test_interrupt_requested = true;
        }
    }

    fn close_graphics_window(&mut self) {
        self.graphics_window = None;
        self.graphics_window_suppressed = false;
        self.graphics_window_dirty = false;
        self.last_frame_command_present = None;
        self.graphics_window_used_this_run = false;
    }

    fn eval_color(&mut self, expr: &str) -> BasicResult<i32> {
        color_number_from_value(self.eval_value(expr)?)
    }

    fn eval_color_arguments(&mut self, args: &str) -> BasicResult<i32> {
        let parts = split_arguments(args);
        match parts.len() {
            1 => self.eval_color(&parts[0]),
            3 => {
                let r = self.eval_number(&parts[0])? as i32;
                let g = self.eval_number(&parts[1])? as i32;
                let b = self.eval_number(&parts[2])? as i32;
                rgb_number(r, g, b)
            }
            _ => Err(self.err(ErrorCode::ArgumentMismatch)),
        }
    }

    fn eval_optional_color(&mut self, parts: &[String], index: usize) -> BasicResult<Option<i32>> {
        parts
            .get(index)
            .map(|expr| self.eval_color(expr))
            .transpose()
    }

    fn eval_optional_paper_color(
        &mut self,
        parts: &[String],
        index: usize,
    ) -> BasicResult<Option<i32>> {
        let Some(expr) = parts.get(index) else {
            return Ok(None);
        };
        let value = self.eval_value(expr)?;
        if matches!(value, Value::Number(n) if n < 0.0) {
            return Ok(Some(-1));
        }
        color_number_from_value(value).map(Some)
    }

    fn execute_screen(&mut self, arg: &str) -> BasicResult<()> {
        let arg = arg.trim();
        if arg.is_empty() {
            self.ensure_graphics_window()?;
            self.graphics.reset_window_state_preserving_buffer();
            self.graphics_window_suppressed = false;
            return self.present_graphics_window();
        }
        if arg.eq_ignore_ascii_case("CLOSE") {
            self.graphics.reset_state();
            self.close_graphics_window();
            return Ok(());
        }
        let value = self.eval_value(arg)?;
        self.graphics.restore_screen(&value.into_string()?)?;
        self.graphics_window_suppressed = false;
        self.present_graphics_window()
    }

    fn execute_plot(&mut self, args: &str, relative: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if !(2..=3).contains(&parts.len()) {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let x = self.eval_number(&parts[0])?;
        let y = self.eval_number(&parts[1])?;
        let color = self.eval_optional_color(&parts, 2)?;
        if relative {
            self.graphics
                .plot(self.graphics.xpos() + x, self.graphics.ypos() + y, color);
        } else {
            self.graphics.plot(x, y, color);
        }
        self.refresh_graphics_window()
    }

    fn execute_move(&mut self, args: &str, relative: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let nums = self.eval_numbers(args)?;
        if nums.len() != 2 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        if relative {
            self.graphics.move_to(
                self.graphics.xpos() + nums[0],
                self.graphics.ypos() + nums[1],
            );
        } else {
            self.graphics.move_to(nums[0], nums[1]);
        }
        Ok(())
    }

    fn execute_mouse(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if parts.len() != 1 || parts[0].trim().is_empty() {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let token = parts[0].trim();
        let visible = if token.eq_ignore_ascii_case("ON") {
            true
        } else if token.eq_ignore_ascii_case("OFF") {
            false
        } else {
            let value = self.eval_number(token)?;
            if value.fract() != 0.0 || !matches!(value as i32, 0 | 1) {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            value as i32 == 1
        };
        if let Some(window) = self.graphics_window.as_mut() {
            window.set_mouse_cursor_visible(visible);
        }
        Ok(())
    }

    fn execute_draw(&mut self, args: &str, relative: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if relative {
            if !(2..=3).contains(&parts.len()) {
                return Err(self.err(ErrorCode::ArgumentMismatch));
            }
            let x = self.eval_number(&parts[0])?;
            let y = self.eval_number(&parts[1])?;
            let color = self.eval_optional_color(&parts, 2)?;
            self.graphics
                .draw_to(self.graphics.xpos() + x, self.graphics.ypos() + y, color);
        } else {
            match parts.len() {
                2 | 3 => {
                    let x = self.eval_number(&parts[0])?;
                    let y = self.eval_number(&parts[1])?;
                    let color = self.eval_optional_color(&parts, 2)?;
                    self.graphics.draw_to(x, y, color);
                }
                4 | 5 => {
                    let x1 = self.eval_number(&parts[0])?;
                    let y1 = self.eval_number(&parts[1])?;
                    let x2 = self.eval_number(&parts[2])?;
                    let y2 = self.eval_number(&parts[3])?;
                    let color = self.eval_optional_color(&parts, 4)?;
                    self.graphics.line_between(x1, y1, x2, y2, color);
                }
                _ => return Err(self.err(ErrorCode::ArgumentMismatch)),
            }
        }
        self.refresh_graphics_window()
    }

    fn execute_rect(&mut self, args: &str, filled: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if !(4..=5).contains(&parts.len()) {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let x1 = self.eval_number(&parts[0])?;
        let y1 = self.eval_number(&parts[1])?;
        let x2 = self.eval_number(&parts[2])?;
        let y2 = self.eval_number(&parts[3])?;
        let color = self.eval_optional_color(&parts, 4)?;
        self.graphics.rectangle(x1, y1, x2, y2, color, filled);
        self.refresh_graphics_window()
    }

    fn execute_triangle(&mut self, args: &str, filled: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if !(6..=7).contains(&parts.len()) {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let x0 = self.eval_number(&parts[0])?;
        let y0 = self.eval_number(&parts[1])?;
        let x1 = self.eval_number(&parts[2])?;
        let y1 = self.eval_number(&parts[3])?;
        let x2 = self.eval_number(&parts[4])?;
        let y2 = self.eval_number(&parts[5])?;
        let color = self.eval_optional_color(&parts, 6)?;
        self.graphics
            .triangle(x0, y0, x1, y1, x2, y2, color, filled);
        self.refresh_graphics_window()
    }

    fn execute_circle(&mut self, args: &str, filled: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if !(3..=7).contains(&parts.len()) {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let x = self.eval_number(&parts[0])?;
        let y = self.eval_number(&parts[1])?;
        let r = self.eval_number(&parts[2])?;
        let color = self.eval_optional_color(&parts, 3)?;
        let (start, end, aspect) = self.eval_circle_tail(&parts)?;
        self.graphics
            .circle_arc(x, y, r, color, filled, start, end, aspect)?;
        self.refresh_graphics_window()
    }

    fn execute_circle_relative(&mut self, args: &str, filled: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if !(3..=7).contains(&parts.len()) {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let x = self.graphics.xpos() + self.eval_number(&parts[0])?;
        let y = self.graphics.ypos() + self.eval_number(&parts[1])?;
        let r = self.eval_number(&parts[2])?;
        let color = self.eval_optional_color(&parts, 3)?;
        let (start, end, aspect) = self.eval_circle_tail(&parts)?;
        self.graphics
            .circle_arc(x, y, r, color, filled, start, end, aspect)?;
        self.refresh_graphics_window()
    }

    fn eval_circle_tail(
        &mut self,
        parts: &[String],
    ) -> BasicResult<(Option<f64>, Option<f64>, f64)> {
        if parts.len() == 5 {
            return Ok((None, None, self.eval_number(&parts[4])?));
        }
        let start = parts
            .get(4)
            .map(|part| self.eval_number(part))
            .transpose()?;
        let end = parts
            .get(5)
            .map(|part| self.eval_number(part))
            .transpose()?;
        let aspect = self.eval_optional_number_or_default(parts, 6, 1.0)?;
        let (start, end) = match (start, end) {
            (Some(start), Some(end)) => {
                if self.angle_degrees {
                    (Some(start.to_radians()), Some(end.to_radians()))
                } else {
                    (Some(start), Some(end))
                }
            }
            _ => (None, None),
        };
        Ok((start, end, aspect))
    }

    fn execute_fill(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        match parts.len() {
            0 => self
                .graphics
                .fill(self.graphics.xpos(), self.graphics.ypos(), None),
            1 => {
                let color = self.eval_color(&parts[0])?;
                self.graphics
                    .fill(self.graphics.xpos(), self.graphics.ypos(), Some(color));
            }
            2 => {
                let x = self.eval_number(&parts[0])?;
                let y = self.eval_number(&parts[1])?;
                self.graphics.fill(x, y, None);
            }
            3 => {
                let x = self.eval_number(&parts[0])?;
                let y = self.eval_number(&parts[1])?;
                let color = self.eval_color(&parts[2])?;
                self.graphics.fill(x, y, Some(color));
            }
            _ => return Err(self.err(ErrorCode::ArgumentMismatch)),
        }
        self.refresh_graphics_window()
    }

    fn execute_scale(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        if args.trim().is_empty() {
            return self.graphics.set_scale(None);
        }
        let nums = self.eval_numbers(args)?;
        if nums.len() < 4 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let border = nums.get(4).copied().unwrap_or(0.0) as i32;
        self.graphics
            .set_scale(Some((nums[0], nums[1], nums[2], nums[3], border)))
    }

    fn execute_graph_range(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        if args.trim().is_empty() {
            return self.graphics.set_graph_range(None);
        }
        let nums = self.eval_numbers(args)?;
        if nums.len() != 4 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        self.graphics
            .set_graph_range(Some((nums[0], nums[1], nums[2], nums[3])))
    }

    fn execute_cross_at(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if args.trim().is_empty() {
            return self.graphics.set_cross_at(None);
        }
        if parts.len() != 2 || parts.iter().any(|part| part.trim().is_empty()) {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let x = self.eval_number(&parts[0])?;
        let y = self.eval_number(&parts[1])?;
        self.graphics.set_cross_at(Some((x, y)))
    }

    fn execute_xaxis(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if parts.len() > 6 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let tic = self.eval_optional_number_or_default(&parts, 0, 1.0)?;
        let force_scientific_labels = parts
            .first()
            .is_some_and(|part| axis_tics_token_is_scientific(part));
        let (scale_xmin, scale_xmax, _, _) = self.graphics.scale_bounds();
        let (xmin, xmax, explicit_range) =
            self.axis_range_from_optional_pair(&parts, 1, 2, scale_xmin, scale_xmax)?;
        let side = self.eval_axis_side_code(&parts, 3)?;
        let orientation = self.eval_axis_binary_flag(&parts, 4)?;
        let subdivisions = self.eval_axis_subdivisions(&parts, 5)?;
        self.graphics.draw_x_axis(
            tic,
            xmin,
            xmax,
            explicit_range,
            side,
            orientation,
            force_scientific_labels,
            subdivisions,
        )?;
        self.refresh_graphics_window()
    }

    fn execute_yaxis(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if parts.len() > 5 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let tic = self.eval_optional_number_or_default(&parts, 0, 1.0)?;
        let force_scientific_labels = parts
            .first()
            .is_some_and(|part| axis_tics_token_is_scientific(part));
        let (_, _, scale_ymin, scale_ymax) = self.graphics.scale_bounds();
        let (ymin, ymax, explicit_range) =
            self.axis_range_from_optional_pair(&parts, 1, 2, scale_ymin, scale_ymax)?;
        let side = self.eval_axis_side_code(&parts, 3)?;
        let subdivisions = self.eval_axis_subdivisions(&parts, 4)?;
        self.graphics.draw_y_axis(
            tic,
            ymin,
            ymax,
            explicit_range,
            side,
            force_scientific_labels,
            subdivisions,
        )?;
        self.refresh_graphics_window()
    }

    fn eval_axis_side_code(&mut self, parts: &[String], index: usize) -> BasicResult<i32> {
        let side = self.eval_optional_number_or_default(parts, index, 0.0)?;
        if !side.is_finite() {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        if side < 0.0 {
            return Ok(-1);
        }
        if side == 0.0 || side == 1.0 {
            return Ok(side as i32);
        }
        Err(self.err(ErrorCode::InvalidArgument))
    }

    fn eval_axis_binary_flag(&mut self, parts: &[String], index: usize) -> BasicResult<i32> {
        let value = self.eval_optional_number_or_default(parts, index, 0.0)?;
        if value == 0.0 || value == 1.0 {
            Ok(value as i32)
        } else {
            Err(self.err(ErrorCode::InvalidArgument))
        }
    }

    fn eval_axis_subdivisions(&mut self, parts: &[String], index: usize) -> BasicResult<i32> {
        let value = self.eval_optional_number_or_default(parts, index, 1.0)?;
        if !value.is_finite() {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let rounded = value.round();
        if (value - rounded).abs() > 1e-9 || rounded < 1.0 {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        Ok(rounded as i32)
    }

    fn execute_graph(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if parts.is_empty() || parts.len() > 2 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let expr_source = self.resolve_graph_expression(&parts[0])?;
        let compiled = compile_expression(&expr_source)?;
        let (xmin, xmax, ymin, ymax) = self.graphics.graph_plot_bounds()?;
        let span_x = xmax - xmin;
        if !span_x.is_finite() || span_x <= 0.0 {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let step = if let Some(raw_step) = parts.get(1).filter(|part| !part.trim().is_empty()) {
            let step = self.eval_number(raw_step)?;
            if !step.is_finite() || step <= 0.0 {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            step.min(span_x)
        } else {
            span_x
                / self
                    .graphics
                    .graph_effective_pixel_width(xmin, xmax, ymin)
                    .max(1) as f64
        };

        let previous_x = self.numeric_variables.get("X").copied();
        let mut mask_phase = 0;
        let mut branch_connected = false;
        let segments = (span_x / step).ceil().max(1.0) as usize;
        let mut x_prev = xmin;
        let mut y_prev = self.eval_graph_y(&compiled, x_prev);
        let mut valid_prev = y_prev.is_some();
        for i in 1..=segments {
            let x_curr = if i == segments {
                xmax
            } else {
                xmin + i as f64 * step
            };
            let y_curr = self.eval_graph_y(&compiled, x_curr);
            let valid_curr = y_curr.is_some();
            match (y_prev, y_curr) {
                (Some(prev_y), Some(curr_y)) if valid_prev && valid_curr => {
                    let x_mid = 0.5 * (x_prev + x_curr);
                    let y_mid = self.eval_graph_y(&compiled, x_mid);
                    if graph_is_discontinuity_bridge(prev_y, curr_y, y_mid, ymin, ymax) {
                        mask_phase = 0;
                        branch_connected = false;
                    } else if y_mid.is_none() {
                        let (lx, ly) =
                            self.refine_graph_valid_endpoint(&compiled, x_prev, prev_y, x_mid);
                        let (rx, ry) =
                            self.refine_graph_valid_endpoint(&compiled, x_curr, curr_y, x_mid);
                        let _ = self.draw_graph_segment_clipped(
                            x_prev,
                            prev_y,
                            lx,
                            ly,
                            (xmin, xmax, ymin, ymax),
                            mask_phase,
                            branch_connected,
                        );

                        mask_phase = 0;
                        branch_connected = false;
                        let (next_phase, consumed) = self.draw_graph_segment_clipped(
                            rx,
                            ry,
                            x_curr,
                            curr_y,
                            (xmin, xmax, ymin, ymax),
                            mask_phase,
                            branch_connected,
                        );
                        mask_phase = next_phase;
                        branch_connected = consumed;
                    } else {
                        let (next_phase, consumed) = self.draw_graph_segment_clipped(
                            x_prev,
                            prev_y,
                            x_curr,
                            curr_y,
                            (xmin, xmax, ymin, ymax),
                            mask_phase,
                            branch_connected,
                        );
                        mask_phase = next_phase;
                        branch_connected = consumed;
                    }
                }
                (Some(prev_y), None) if valid_prev => {
                    let (bx, by) =
                        self.refine_graph_valid_endpoint(&compiled, x_prev, prev_y, x_curr);
                    let _ = self.draw_graph_segment_clipped(
                        x_prev,
                        prev_y,
                        bx,
                        by,
                        (xmin, xmax, ymin, ymax),
                        mask_phase,
                        branch_connected,
                    );
                    mask_phase = 0;
                    branch_connected = false;
                }
                (None, Some(curr_y)) if valid_curr => {
                    let (bx, by) =
                        self.refine_graph_valid_endpoint(&compiled, x_curr, curr_y, x_prev);
                    mask_phase = 0;
                    branch_connected = false;
                    let (next_phase, consumed) = self.draw_graph_segment_clipped(
                        bx,
                        by,
                        x_curr,
                        curr_y,
                        (xmin, xmax, ymin, ymax),
                        mask_phase,
                        branch_connected,
                    );
                    mask_phase = next_phase;
                    branch_connected = consumed;
                }
                _ => {}
            }
            x_prev = x_curr;
            y_prev = y_curr;
            valid_prev = valid_curr;
        }
        if let Some(value) = previous_x {
            self.numeric_variables.insert("X".to_string(), value);
        } else {
            self.numeric_variables.remove("X");
        }
        self.refresh_graphics_window()
    }

    fn eval_optional_number_or_default(
        &mut self,
        parts: &[String],
        index: usize,
        default: f64,
    ) -> BasicResult<f64> {
        let Some(part) = parts.get(index) else {
            return Ok(default);
        };
        if part.trim().is_empty() {
            Ok(default)
        } else {
            self.eval_number(part)
        }
    }

    fn axis_range_from_optional_pair(
        &mut self,
        parts: &[String],
        first_index: usize,
        second_index: usize,
        default_min: f64,
        default_max: f64,
    ) -> BasicResult<(f64, f64, bool)> {
        let first = parts
            .get(first_index)
            .map(|part| part.trim())
            .unwrap_or_default();
        let second = parts
            .get(second_index)
            .map(|part| part.trim())
            .unwrap_or_default();
        match (first.is_empty(), second.is_empty()) {
            (true, true) => Ok((default_min, default_max, false)),
            (false, false) => Ok((self.eval_number(first)?, self.eval_number(second)?, true)),
            _ => Err(self.err(ErrorCode::ArgumentMismatch)),
        }
    }

    fn resolve_graph_expression(&mut self, expr: &str) -> BasicResult<String> {
        let raw = expr.trim();
        if raw.is_empty() {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        if raw.starts_with('"') || raw.contains('$') {
            let value = self.eval_value(raw)?;
            let text = value.into_string()?.trim().to_string();
            if text.is_empty() {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            Ok(text)
        } else {
            Ok(raw.to_string())
        }
    }

    fn eval_graph_y(&mut self, expr: &Expr, x: f64) -> Option<f64> {
        self.numeric_variables.insert("X".to_string(), x);
        eval_compiled_number(self, expr)
            .ok()
            .filter(|value| value.is_finite())
    }

    fn refine_graph_valid_endpoint(
        &mut self,
        expr: &Expr,
        x_valid: f64,
        y_valid: f64,
        x_invalid: f64,
    ) -> (f64, f64) {
        let mut lx = x_valid;
        let mut ly = y_valid;
        let mut rx = x_invalid;
        for _ in 0..24 {
            let mx = 0.5 * (lx + rx);
            if mx == lx || mx == rx {
                break;
            }
            if let Some(my) = self.eval_graph_y(expr, mx) {
                lx = mx;
                ly = my;
            } else {
                rx = mx;
            }
        }
        (lx, ly)
    }

    fn draw_graph_segment_clipped(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        bounds: (f64, f64, f64, f64),
        mask_phase: u8,
        skip_first_pixel: bool,
    ) -> (u8, bool) {
        let (xmin, xmax, ymin, ymax) = bounds;
        let Some((cx0, cy0, cx1, cy1)) = clip_graph_segment(x0, y0, x1, y1, xmin, xmax, ymin, ymax)
        else {
            return (mask_phase, false);
        };
        self.graphics.line_between_with_mask_phase(
            cx0,
            cy0,
            cx1,
            cy1,
            None,
            mask_phase,
            skip_first_pixel,
        )
    }

    fn execute_disp(&mut self, args: &str, graphics_coords: bool) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if parts.is_empty() {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let text = self.eval_value(&parts[0])?.into_string()?;
        let ink = parts.get(1).map(|s| self.eval_color(s)).transpose()?;
        let paper = self.eval_optional_paper_color(&parts, 2)?;
        if graphics_coords {
            self.graphics.gdisp(&text, ink, paper);
        } else {
            self.graphics.disp(&text, ink, paper);
        }
        self.refresh_graphics_window()
    }

    fn execute_sprite(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let upper = args.to_ascii_uppercase();
        if upper.starts_with("DEL") {
            let id = self.eval_number(args[3..].trim())? as i32;
            self.graphics.sprite_delete(id);
            return Ok(());
        }
        if upper.starts_with("MOVE") {
            let nums = split_arguments(args[4..].trim());
            if nums.len() < 3 {
                return Err(self.err(ErrorCode::ArgumentMismatch));
            }
            let id = self.eval_number(&nums[0])? as i32;
            let x = self.eval_number(&nums[1])?;
            let y = self.eval_number(&nums[2])?;
            let transparent = nums.get(3).map(|s| self.eval_color(s)).transpose()?;
            self.graphics.sprite_move(id, x, y, transparent)?;
            return self.refresh_graphics_window();
        }
        let hittest = upper.starts_with("HITTEST");
        let body = if hittest { args[7..].trim() } else { args };
        let parts = split_arguments(body);
        if parts.len() < 3 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let sprite = self.eval_value(&parts[0])?.into_string()?;
        let x = self.eval_number(&parts[1])?;
        let y = self.eval_number(&parts[2])?;
        let transparent = parts.get(3).map(|s| self.eval_color(s)).transpose()?;
        let id = parts
            .get(4)
            .map(|s| self.eval_number(s).map(|n| n as i32))
            .transpose()?;
        self.graphics
            .draw_sprite(&sprite, x, y, transparent, id, hittest)?;
        if hittest {
            Ok(())
        } else {
            self.refresh_graphics_window()
        }
    }

    fn execute_bsave(&mut self, args: &str) -> BasicResult<()> {
        self.ensure_graphics_window()?;
        let parts = split_arguments(args);
        if parts.is_empty() {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let path = self.resolve_path_value(&parts[0])?;
        if parts.len() == 1 {
            self.graphics.save_png(&path)
        } else {
            let screen = self.eval_value(&parts[1])?.into_string()?;
            let mut temp = Graphics::new(640);
            temp.restore_screen(&screen)?;
            temp.save_png(&path)
        }
    }

    fn execute_bload(&mut self, args: &str) -> BasicResult<()> {
        let parts = split_arguments(args);
        if parts.is_empty() {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let path = self.resolve_read_path_value(&parts[0])?;
        let gscr = Graphics::load_png_to_gscr(&path)?;
        if parts.len() == 1 {
            self.graphics.restore_screen(&gscr)?;
            self.graphics_window_suppressed = false;
            self.present_graphics_window()
        } else {
            self.assign(&parts[1], Value::string(gscr))
        }
    }

    fn execute_merge(&mut self, args: &str) -> BasicResult<()> {
        let parts = split_arguments(args);
        if parts.is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }
        let path = self.resolve_bas_path_expr(&parts[0])?;
        let text = read_text_file(&path)?;
        self.program.merge_text(&text)?;
        self.refresh_identifier_case_from_program();
        self.rebuild_data();
        self.rebuild_command_cache();
        self.expression_cache.clear();
        self.program_structure_changed = true;
        Ok(())
    }

    fn execute_chain(&mut self, args: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let parts = split_arguments(args);
        if parts.is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }
        let path = self.resolve_bas_path_expr(&parts[0])?;
        let entry = parts
            .get(1)
            .map(|part| self.eval_number(part).map(|n| n as i32))
            .transpose()?;
        self.load_file_preserving_runtime(&path)?;
        let line_idx = if let Some(line) = entry {
            self.line_index(line)
                .ok_or_else(|| self.err(ErrorCode::TargetLineNotFound))?
        } else {
            0
        };
        *cursor = Cursor {
            line_idx,
            cmd_idx: 0,
        };
        self.restart_run_loop = true;
        Ok(())
    }

    fn execute_chain_merge(&mut self, args: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let parts = split_arguments(args);
        if parts.is_empty() {
            return Err(self.err(ErrorCode::Syntax));
        }
        let path = self.resolve_bas_path_expr(&parts[0])?;
        let entry = parts
            .get(1)
            .map(|part| self.eval_number(part).map(|n| n as i32))
            .transpose()?;
        for part in parts.iter().skip(2) {
            let trimmed = part.trim();
            if trimmed.to_ascii_uppercase().starts_with("DELETE") {
                let range = trimmed[6..].trim();
                let (start, end) = parse_delete_range(range)?;
                self.program.delete_range(start, end);
            }
        }
        let text = read_text_file(&path)?;
        self.program.merge_text(&text)?;
        self.refresh_identifier_case_from_program();
        self.rebuild_data();
        self.data_pointer = 0;
        self.rebuild_command_cache();
        self.expression_cache.clear();
        let line_idx = if let Some(line) = entry {
            self.line_index(line)
                .ok_or_else(|| self.err(ErrorCode::TargetLineNotFound))?
        } else {
            0
        };
        *cursor = Cursor {
            line_idx,
            cmd_idx: 0,
        };
        self.restart_run_loop = true;
        Ok(())
    }

    fn load_file_preserving_runtime(&mut self, path: &Path) -> BasicResult<()> {
        let text = read_text_file(path)?;
        self.program.load_text(&text)?;
        self.clear_command_caches();
        self.program_dir = path.parent().map(Path::to_path_buf);
        self.refresh_identifier_case_from_program();
        self.functions.clear();
        self.subs.clear();
        self.fn_line_owner.clear();
        self.sub_line_owner.clear();
        self.rebuild_data();
        self.data_pointer = 0;
        self.rebuild_command_cache();
        self.expression_cache.clear();
        Ok(())
    }

    fn jump_to(&mut self, target_expr: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let line = parse_line_number_literal(target_expr)
            .ok_or_else(|| self.err(ErrorCode::InvalidLineNumber))?;
        self.jump_to_line_checked(line, cursor, false)
    }

    fn jump_to_gosub(&mut self, target_expr: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let line = parse_line_number_literal(target_expr)
            .ok_or_else(|| self.err(ErrorCode::InvalidLineNumber))?;
        self.jump_to_line_checked(line, cursor, true)
    }

    fn jump_to_line(&mut self, line: i32, cursor: &mut Cursor) -> BasicResult<()> {
        self.jump_to_line_checked(line, cursor, false)
    }

    fn jump_to_line_checked(
        &mut self,
        line: i32,
        cursor: &mut Cursor,
        allow_function_subroutine: bool,
    ) -> BasicResult<()> {
        self.jump_to_cached_line_checked(line, None, cursor, allow_function_subroutine)
    }

    fn jump_to_cached_line_checked(
        &mut self,
        line: i32,
        cached_target: Option<&Cursor>,
        cursor: &mut Cursor,
        allow_function_subroutine: bool,
    ) -> BasicResult<()> {
        let target = if let Some(target) = cached_target {
            target.clone()
        } else {
            let Some(index) = self.line_index(line) else {
                return Err(self.err(ErrorCode::TargetLineNotFound));
            };
            Cursor {
                line_idx: index,
                cmd_idx: 0,
            }
        };
        if self.can_fast_jump_to_line(line) {
            if target == *cursor {
                self.repeat_current_command = true;
            }
            *cursor = target;
            return Ok(());
        }
        self.validate_function_jump(line, &target, allow_function_subroutine)?;
        if !self.if_stack.is_empty() {
            self.reconcile_if_stack_for_jump(&target)?;
        }
        if target == *cursor {
            self.repeat_current_command = true;
        }
        *cursor = target;
        Ok(())
    }

    fn can_fast_jump_to_line(&self, line: i32) -> bool {
        self.if_stack.is_empty()
            && self.active_functions.is_empty()
            && self.active_subs.is_empty()
            && self.fn_line_owner.get(&line).is_none()
            && self.sub_line_owner.get(&line).is_none()
    }

    fn jump_to_line_unchecked(&mut self, line: i32, cursor: &mut Cursor) -> BasicResult<()> {
        let Some(index) = self.line_index(line) else {
            return Err(self.err(ErrorCode::TargetLineNotFound));
        };
        *cursor = Cursor {
            line_idx: index,
            cmd_idx: 0,
        };
        Ok(())
    }

    fn jump_to_line_for_resume(&mut self, line: i32, cursor: &mut Cursor) -> BasicResult<()> {
        let Some(index) = self.line_index(line) else {
            return Err(self.err(ErrorCode::TargetLineNotFound));
        };
        let target = Cursor {
            line_idx: index,
            cmd_idx: 0,
        };
        if let Some(owner) = self.function_for_line(line) {
            if self.active_function_name() != Some(owner) {
                return Err(self.err(ErrorCode::InvalidTargetLine));
            }
        }
        if let Some(owner) = self.sub_for_line(line) {
            if self.active_sub_name() != Some(owner) {
                return Err(self.err(ErrorCode::InvalidTargetLine));
            }
        }
        self.reconcile_if_stack_for_jump(&target)?;
        *cursor = target;
        Ok(())
    }

    fn handle_runtime_error(
        &mut self,
        err: BasicError,
        cursor: &mut Cursor,
        retry: Cursor,
        next: Cursor,
    ) -> BasicResult<bool> {
        if err
            .detail
            .as_deref()
            .is_some_and(|detail| detail.starts_with("Error in error handler:"))
        {
            return Err(err);
        }
        let line = err.line.or(self.current_line).unwrap_or(0);
        let number = basic_error_number(&err);
        self.last_error = Some(RuntimeErrorState {
            number,
            line,
            retry,
            next: self.cursor_after_cached_command(next),
        });
        if self.handling_error {
            return Err(BasicError::new(err.code).with_detail(format!(
                "Error in error handler: {}",
                error_message_without_dot(&err)
            )));
        }
        if self.error_resume_next {
            if let Some(state) = &self.last_error {
                *cursor = state.next.clone();
            }
            return Ok(true);
        }
        if let Some(handler) = self.error_handler_line {
            self.handling_error = true;
            self.jump_to_line_unchecked(handler, cursor)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn validate_function_jump(
        &self,
        line: i32,
        _target: &Cursor,
        allow_function_subroutine: bool,
    ) -> BasicResult<()> {
        let target_owner = self.function_for_line(line);
        let target_sub_owner = self.sub_for_line(line);
        if let Some(active) = self.active_function_name() {
            if let Some(owner) = target_owner {
                if owner != active {
                    return Err(self.err(ErrorCode::InvalidTargetLine));
                }
                return Ok(());
            }
            if target_sub_owner.is_some() {
                return Err(self.err(ErrorCode::InvalidTargetLine));
            }
            if allow_function_subroutine {
                return Ok(());
            }
            return Err(self.err(ErrorCode::InvalidTargetLine));
        }
        if let Some(active) = self.active_sub_name() {
            if let Some(owner) = target_sub_owner {
                if owner != active {
                    return Err(self.err(ErrorCode::InvalidTargetLine));
                }
                return Ok(());
            }
            if target_owner.is_some() {
                return Err(self.err(ErrorCode::InvalidTargetLine));
            }
            if allow_function_subroutine {
                return Ok(());
            }
            return Err(self.err(ErrorCode::InvalidTargetLine));
        }
        if target_owner.is_some() || target_sub_owner.is_some() {
            return Err(self.err(ErrorCode::InvalidTargetLine));
        }
        Ok(())
    }

    fn reconcile_if_stack_for_jump(&mut self, target: &Cursor) -> BasicResult<()> {
        let mut keep = Vec::new();
        for frame in self.if_stack.iter() {
            let after = self.find_after_matching_end_if(frame)?;
            if cursor_after(target, frame) && cursor_before(target, &after) {
                keep.push(frame.clone());
            }
        }
        self.if_stack = keep;
        Ok(())
    }

    fn rebuild_data(&mut self) {
        self.data.clear();
        self.data_line_starts.clear();
        for line in self.program.line_numbers() {
            if let Some(code) = self.program.get(line) {
                for command in split_commands(code) {
                    if command
                        .trim_start()
                        .to_ascii_uppercase()
                        .starts_with("DATA")
                    {
                        self.data_line_starts.entry(line).or_insert(self.data.len());
                        self.data
                            .extend(parse_data_items(command.trim_start()[4..].trim()));
                    }
                }
            }
        }
    }

    fn clear_command_caches(&mut self) {
        self.command_cache.clear();
        self.compiled_command_cache.clear();
        self.line_index_cache.clear();
        self.next_after_for_cache.clear();
        self.wend_after_while_cache.clear();
    }

    fn rebuild_command_cache(&mut self) {
        self.clear_command_caches();
        for (idx, line) in self.program.line_numbers().into_iter().enumerate() {
            let commands: Vec<Rc<str>> = split_commands(self.program.get(line).unwrap_or(""))
                .into_iter()
                .map(Rc::<str>::from)
                .collect();
            let compiled = commands
                .iter()
                .map(|command| Rc::new(self.compile_cached_command(command.as_ref())))
                .collect();
            self.command_cache.insert(line, commands);
            self.compiled_command_cache.insert(line, compiled);
            self.line_index_cache.insert(line, idx);
        }
        self.rebuild_block_target_caches();
    }

    fn rebuild_block_target_caches(&mut self) {
        let lines = self.program.line_numbers();
        let mut for_stack = Vec::new();
        let mut while_stack = Vec::new();
        for (line_idx, line_no) in lines.into_iter().enumerate() {
            let Some(commands) = self.command_cache.get(&line_no) else {
                continue;
            };
            for (cmd_idx, command) in commands.iter().enumerate() {
                let first = command
                    .trim_start()
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                match first.as_str() {
                    "FOR" => for_stack.push(Cursor { line_idx, cmd_idx }),
                    "NEXT" => {
                        if let Some(start) = for_stack.pop() {
                            self.next_after_for_cache.insert(
                                start,
                                Cursor {
                                    line_idx,
                                    cmd_idx: cmd_idx + 1,
                                },
                            );
                        }
                    }
                    "WHILE" => while_stack.push(Cursor { line_idx, cmd_idx }),
                    "WEND" => {
                        if let Some(start) = while_stack.pop() {
                            self.wend_after_while_cache.insert(
                                start,
                                Cursor {
                                    line_idx,
                                    cmd_idx: cmd_idx + 1,
                                },
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn compile_cached_command(&mut self, command: &str) -> CachedCommand {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return CachedCommand::Noop;
        }
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("MID$(") {
            return compile_mid_assignment_statement(trimmed)
                .map(|compiled| CachedCommand::MidAssignment(Rc::new(compiled)))
                .unwrap_or_else(|_| CachedCommand::Raw(Rc::<str>::from(trimmed)));
        }
        let first = upper.split_whitespace().next().unwrap_or("");
        match first {
            "REM" | "DATA" => CachedCommand::Noop,
            "RETURN" if upper == "RETURN" => CachedCommand::Return,
            "IF" => self
                .compile_cached_if(trimmed)
                .unwrap_or_else(|| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            "FOR" => compile_for_statement(trimmed[3..].trim())
                .map(|compiled| CachedCommand::For(Rc::new(compiled)))
                .unwrap_or_else(|_| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            "ON" if !upper.starts_with("ON ERROR") => self
                .compile_cached_on(trimmed)
                .unwrap_or_else(|| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            "GOTO" => parse_line_number_literal(trimmed[4..].trim())
                .map(|line| CachedCommand::GotoConst {
                    line,
                    target: self.cursor_for_line(line),
                })
                .unwrap_or_else(|| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            "GOSUB" => parse_line_number_literal(trimmed[5..].trim())
                .map(|line| CachedCommand::GosubConst {
                    line,
                    target: self.cursor_for_line(line),
                })
                .unwrap_or_else(|| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            "NEXT" => {
                let arg = trimmed[4..].trim();
                if arg.is_empty() {
                    CachedCommand::Next(None)
                } else if is_basic_identifier(arg) {
                    CachedCommand::Next(Some(arg.to_ascii_uppercase()))
                } else {
                    CachedCommand::Raw(Rc::<str>::from(trimmed))
                }
            }
            "WHILE" => compile_expression(trimmed[5..].trim())
                .map(|expr| CachedCommand::While(Rc::new(expr)))
                .unwrap_or_else(|_| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            "WEND" if upper == "WEND" => CachedCommand::Wend,
            "DRAWR" => compile_draw_relative2(trimmed[5..].trim())
                .map(|(x, y)| CachedCommand::DrawRelative2 {
                    x: Rc::new(x),
                    y: Rc::new(y),
                })
                .unwrap_or_else(|_| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            "LET" => self
                .compile_cached_assignment(trimmed[3..].trim())
                .unwrap_or_else(|| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            _ if is_assignment(trimmed) => self
                .compile_cached_assignment(trimmed)
                .unwrap_or_else(|| CachedCommand::Raw(Rc::<str>::from(trimmed))),
            _ => CachedCommand::Raw(Rc::<str>::from(trimmed)),
        }
    }

    fn compile_cached_if(&mut self, command: &str) -> Option<CachedCommand> {
        let upper = command.to_ascii_uppercase();
        let (cond, rest) = if let Some((then_pos, then_end)) = find_then_keyword(&upper) {
            (command[2..then_pos].trim(), command[then_end..].trim())
        } else if let Some(goto_pos) = find_keyword_after_if(&upper, " GOTO ") {
            (command[2..goto_pos].trim(), command[goto_pos + 1..].trim())
        } else {
            return None;
        };
        if rest.is_empty() {
            return None;
        }
        let condition = Rc::new(compile_expression(cond).ok()?);
        let (then_part, else_part) = split_else(rest);
        let then_branch = self.compile_cached_if_branch(then_part)?;
        let else_branch = if let Some(branch) = else_part {
            Some(self.compile_cached_if_branch(branch)?)
        } else {
            None
        };
        Some(CachedCommand::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn compile_cached_if_branch(&mut self, branch: &str) -> Option<CachedIfBranch> {
        let trimmed = branch.trim();
        if trimmed.is_empty() {
            return Some(CachedIfBranch::Commands(Vec::new()));
        }
        if let Ok(line) = trimmed.parse::<i32>() {
            return Some(CachedIfBranch::Line(line));
        }
        let subcommands = split_commands(trimmed);
        if subcommands
            .iter()
            .enumerate()
            .any(|(idx, command)| idx + 1 < subcommands.len() && first_word_is(command, "GOSUB"))
        {
            return None;
        }
        Some(CachedIfBranch::Commands(
            subcommands
                .iter()
                .map(|command| Rc::new(self.compile_cached_command(command)))
                .collect(),
        ))
    }

    fn compile_cached_on(&mut self, command: &str) -> Option<CachedCommand> {
        let body = command[2..].trim();
        let upper = body.to_ascii_uppercase();
        let (gosub, pos, keyword_len) = if let Some(pos) = upper.find(" GOSUB ") {
            (true, pos, 7)
        } else if let Some(pos) = upper.find(" GOTO ") {
            (false, pos, 6)
        } else {
            return None;
        };
        let selector = Rc::new(compile_expression(body[..pos].trim()).ok()?);
        let targets = split_arguments(body[pos + keyword_len..].trim())
            .iter()
            .map(|target| parse_line_number_literal(target))
            .collect::<Option<Vec<_>>>()?;
        if gosub {
            Some(CachedCommand::OnGosub { selector, targets })
        } else {
            Some(CachedCommand::OnGoto { selector, targets })
        }
    }

    fn compile_cached_assignment(&mut self, source: &str) -> Option<CachedCommand> {
        let key = source.trim();
        if key.is_empty()
            || contains_double_equal_top_level(key)
            || key.trim_start().to_ascii_uppercase().starts_with("MID$(")
        {
            return None;
        }
        if let Some(compiled) = self.assignment_cache.get(key) {
            if let Some((target, source, index)) =
                compiled_string_char_assignment(compiled.as_ref())
            {
                return Some(CachedCommand::StringCharAssignment {
                    target,
                    source,
                    index: Rc::new(index),
                });
            }
            return Some(CachedCommand::Assignment(compiled.clone()));
        }
        let compiled = Rc::new(compile_assignment_statement(key).ok()?);
        if let Some((target, source, index)) = compiled_string_char_assignment(compiled.as_ref()) {
            return Some(CachedCommand::StringCharAssignment {
                target,
                source,
                index: Rc::new(index),
            });
        }
        self.assignment_cache
            .insert(key.to_string(), compiled.clone());
        Some(CachedCommand::Assignment(compiled))
    }

    fn line_index(&self, line: i32) -> Option<usize> {
        self.line_index_cache
            .get(&line)
            .copied()
            .or_else(|| self.program.index_of(line))
    }

    fn cursor_for_line(&self, line: i32) -> Option<Cursor> {
        self.line_index(line).map(|line_idx| Cursor {
            line_idx,
            cmd_idx: 0,
        })
    }

    fn eval_value(&mut self, expr: &str) -> BasicResult<Value> {
        let key = expr.trim();
        let compiled = if let Some(compiled) = self.expression_cache.get(key) {
            compiled.clone()
        } else {
            let compiled =
                Rc::new(compile_expression(&key).map_err(|e| self.with_current_line(e))?);
            self.expression_cache
                .insert(key.to_string(), compiled.clone());
            compiled
        };
        eval_compiled(self, compiled.as_ref()).map_err(|mut e| {
            if let Some(line) = self.current_line {
                if e.line.is_none() {
                    e.line = Some(line);
                }
            }
            e
        })
    }

    fn eval_number(&mut self, expr: &str) -> BasicResult<f64> {
        let key = expr.trim();
        let compiled = if let Some(compiled) = self.expression_cache.get(key) {
            compiled.clone()
        } else {
            let compiled =
                Rc::new(compile_expression(&key).map_err(|e| self.with_current_line(e))?);
            self.expression_cache
                .insert(key.to_string(), compiled.clone());
            compiled
        };
        eval_compiled_number(self, compiled.as_ref()).map_err(|mut e| {
            if let Some(line) = self.current_line {
                if e.line.is_none() {
                    e.line = Some(line);
                }
            }
            e
        })
    }

    fn eval_numbers(&mut self, args: &str) -> BasicResult<Vec<f64>> {
        split_arguments(args)
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| self.eval_number(&s))
            .collect()
    }

    fn resolve_bas_literal_arg(&self, arg: &str) -> BasicResult<PathBuf> {
        let path = extract_quoted_path_text(arg)?;
        self.resolve_bas_path_text(&path, &self.effective_base_dir())
    }

    fn resolve_bas_path_expr(&mut self, expr: &str) -> BasicResult<PathBuf> {
        let trimmed = expr.trim();
        if trimmed.starts_with('"') {
            let path = extract_quoted_path_text(trimmed)?;
            return self.resolve_bas_path_text(&path, &self.effective_base_dir());
        }
        let value = self.eval_value(trimmed)?;
        let Value::Str(path) = value else {
            return Err(self.err(ErrorCode::TypeMismatch));
        };
        if path.contains('"') {
            return Err(self.err(ErrorCode::Syntax));
        }
        self.resolve_bas_path_text(&path, &self.effective_base_dir())
    }

    fn resolve_bas_path_text(&self, path: &str, base_dir: &Path) -> BasicResult<PathBuf> {
        let mut resolved = self.resolve_virtual_path_text(path, base_dir)?;
        if resolved.extension().is_none() {
            resolved.set_extension("bas");
        } else if !resolved
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bas"))
        {
            return Err(self.err(ErrorCode::OnlyBasFiles));
        }
        if !resolved.exists() {
            if let Ok(relative) = resolved.strip_prefix(self.canonical_root_dir()) {
                if let Some(oracle_root) = self.oracle_root_dir() {
                    let oracle_path = oracle_root.join(relative);
                    if oracle_path.exists() {
                        return Ok(oracle_path);
                    }
                }
            }
        }
        Ok(resolved)
    }

    fn resolve_path_value(&mut self, expr: &str) -> BasicResult<PathBuf> {
        let value = self.eval_value(expr)?;
        let Value::Str(path) = value else {
            return Err(self.err(ErrorCode::TypeMismatch));
        };
        Ok(self.effective_base_dir().join(path))
    }

    fn resolve_read_path_value(&mut self, expr: &str) -> BasicResult<PathBuf> {
        let value = self.eval_value(expr)?;
        let Value::Str(path) = value else {
            return Err(self.err(ErrorCode::TypeMismatch));
        };
        let raw = PathBuf::from(&path);
        if raw.is_absolute() {
            return Ok(raw);
        }
        let local = self.effective_base_dir().join(&path);
        if local.exists() {
            return Ok(local);
        }
        if let Some(oracle_root) = self.oracle_root_dir() {
            let python_repo = oracle_root.join(&path);
            if python_repo.exists() {
                return Ok(python_repo);
            }
        }
        Ok(local)
    }

    fn effective_base_dir(&self) -> PathBuf {
        self.program_dir
            .clone()
            .unwrap_or_else(|| self.current_dir.clone())
    }

    fn render_program_list_range(&self, args: &str) -> BasicResult<String> {
        let lines = self.program.line_numbers();
        if args.trim().is_empty() {
            return Ok(self.render_program_lines(lines));
        }
        let range = parse_line_range_spec(args, ErrorCode::Syntax, ErrorCode::TypeMismatch)?;
        if let LineRangeSpec::Single(line) = range {
            if !lines.contains(&line) {
                return Err(self.err(ErrorCode::TargetLineNotFound));
            }
            return Ok(self.render_program_lines(vec![line]));
        }
        let (start, end) = range.bounds_for(&lines);
        Ok(self.render_program_lines(
            lines
                .into_iter()
                .filter(|line| *line >= start && *line <= end)
                .collect(),
        ))
    }

    fn render_program_lines(&self, lines: Vec<i32>) -> String {
        let mut out = String::new();
        for line in lines {
            let code = self.program.get(line).unwrap_or("");
            let code = apply_identifier_case(code, &self.identifier_case);
            let line_text = format!("{line}{code}");
            if self.ansi_output {
                out.push_str(&console::syntax_highlight_with_cases(
                    &line_text,
                    true,
                    Some(&self.identifier_case),
                ));
                out.push('\n');
            } else {
                out.push_str(&line_text);
                out.push('\n');
            }
        }
        out
    }

    fn write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.stream_output {
            print!("{text}");
        } else {
            self.output.push_str(text);
        }
        self.track_output_position(text);
    }

    fn write_line(&mut self, text: &str) {
        if self.stream_output {
            println!("{text}");
            let _ = io::stdout().flush();
        } else {
            self.output.push_str(text);
            self.output.push('\n');
        }
        self.line_open = false;
        self.output_col = 0;
    }

    fn flush_stream_output(&self) {
        if self.stream_output {
            let _ = io::stdout().flush();
        }
    }

    fn finish_output_line(&mut self) {
        if self.line_open {
            self.write_line("");
        }
    }

    fn track_output_position(&mut self, text: &str) {
        for ch in text.chars() {
            match ch {
                '\n' => {
                    self.output_col = 0;
                    self.line_open = false;
                }
                '\t' => {
                    self.output_col += 8 - (self.output_col % 8);
                    self.line_open = true;
                }
                _ => {
                    self.output_col += 1;
                    self.line_open = true;
                }
            }
        }
    }

    fn read_inkey(&mut self) -> String {
        let use_graphics_keyboard = self.current_run_uses_graphics_window();
        let _ = self.pump_graphics_window_if_due();
        if use_graphics_keyboard {
            if let Some(window) = self.graphics_window.as_mut() {
                if let Some(code) = window.take_key_code() {
                    return char::from_u32(code as u32)
                        .map(|ch| ch.to_string())
                        .unwrap_or_default();
                }
            }
            return String::new();
        }
        self.scan_console_key_once();
        let Some(code) = self.key_queue.pop_front() else {
            return String::new();
        };
        char::from_u32(code as u32)
            .map(|ch| ch.to_string())
            .unwrap_or_default()
    }

    fn key_down(&mut self, code: u8) -> bool {
        let use_graphics_keyboard = self.current_run_uses_graphics_window();
        let _ = self.pump_graphics_window_if_due();
        if use_graphics_keyboard {
            return self
                .graphics_window
                .as_ref()
                .is_some_and(|window| window.key_down_code(code));
        }
        self.scan_console_key_once();
        self.key_queue.iter().any(|queued| *queued == code)
    }

    fn scan_console_key_once(&mut self) {
        if self.key_queue.is_empty() {
            if let Some(code) = console_read_key_code() {
                if code == 3 {
                    console::request_interrupt();
                } else {
                    self.key_queue.push_back(code);
                }
            }
        }
    }

    fn err(&self, code: ErrorCode) -> BasicError {
        let mut err = BasicError::new(code);
        if let Some(line) = self.current_line {
            err.line = Some(line);
        }
        err
    }

    fn with_current_line(&self, mut err: BasicError) -> BasicError {
        if err.line.is_none() {
            err.line = self.current_line;
        }
        err
    }
}

impl EvalContext for Interpreter {
    fn get_variable(&mut self, name: &str) -> BasicResult<Value> {
        if name.ends_with('$') {
            Ok(Value::string(
                self.string_variables.get(name).cloned().unwrap_or_default(),
            ))
        } else {
            if name == "INF" {
                return Ok(Value::number(f64::INFINITY));
            }
            if !self.function_call_stack.is_empty()
                && !self.numeric_variables.contains_key(name)
                && self.arrays.contains_key(name)
            {
                return Ok(Value::ArrayRef(name.to_string()));
            }
            Ok(Value::number(
                self.numeric_variables.get(name).copied().unwrap_or(0.0),
            ))
        }
    }

    fn get_array_value(&mut self, name: &str, indexes: &[i32]) -> BasicResult<Value> {
        if let Some(array) = self.arrays.get(name) {
            if array.dims.len() == indexes.len() {
                if indexes.len() == 1 {
                    return array.get_direct_1d(indexes[0]);
                }
                return array.get(indexes);
            }
        }
        let indexes = self.normalize_array_indexes_for_name(name, indexes.to_vec())?;
        if !self.arrays.contains_key(name) {
            self.arrays.insert(
                name.to_string(),
                ArrayValue::new(name, vec![10; indexes.len()]),
            );
        }
        let array = self.arrays.get(name).unwrap();
        array.get(&indexes)
    }

    fn array_reference(&mut self, name: &str) -> Option<Value> {
        self.arrays
            .contains_key(name)
            .then(|| Value::ArrayRef(name.to_string()))
    }

    fn call_runtime_function(&mut self, name: &str, args: Vec<Value>) -> BasicResult<Value> {
        match name.to_ascii_uppercase().as_str() {
            "SIN" if args.len() == 1 => {
                let mut x = args[0].as_number()?;
                if self.angle_degrees {
                    x = x.to_radians();
                }
                return Ok(Value::number(x.sin()));
            }
            "COS" if args.len() == 1 => {
                let mut x = args[0].as_number()?;
                if self.angle_degrees {
                    x = x.to_radians();
                }
                return Ok(Value::number(x.cos()));
            }
            "TAN" if args.len() == 1 => {
                let mut x = args[0].as_number()?;
                if self.angle_degrees {
                    x = x.to_radians();
                }
                return Ok(Value::number(x.tan()));
            }
            "ASN" if args.len() == 1 => {
                let raw = args[0].as_number()?;
                if !(-1.0..=1.0).contains(&raw) {
                    return Err(self.err(ErrorCode::InvalidArgument));
                }
                let x = raw.asin();
                return Ok(Value::number(if self.angle_degrees {
                    x.to_degrees()
                } else {
                    x
                }));
            }
            "ACS" if args.len() == 1 => {
                let raw = args[0].as_number()?;
                if !(-1.0..=1.0).contains(&raw) {
                    return Err(self.err(ErrorCode::InvalidArgument));
                }
                let x = raw.acos();
                return Ok(Value::number(if self.angle_degrees {
                    x.to_degrees()
                } else {
                    x
                }));
            }
            "ATN" if args.len() == 1 => {
                let x = args[0].as_number()?.atan();
                return Ok(Value::number(if self.angle_degrees {
                    x.to_degrees()
                } else {
                    x
                }));
            }
            "COT" if args.len() == 1 => {
                let mut x = args[0].as_number()?;
                if self.angle_degrees {
                    x = x.to_radians();
                }
                let t = x.tan();
                if t == 0.0 {
                    return Err(self.err(ErrorCode::InvalidArgument));
                }
                return Ok(Value::number(1.0 / t));
            }
            "LBOUND" | "UBOUND" | "LBND" | "UBND" => {
                return self.call_array_bound_function(name, args);
            }
            "DET" => {
                return self.call_det_function(args);
            }
            "ABSUM" | "AMAX" | "AMIN" | "CNORM" | "DOT" | "FNORM" | "MAXAB" | "RNORM" | "SUM" => {
                return self.call_mat_stat_function(name, args);
            }
            "TRN" | "INV" => {
                return Err(self.err(ErrorCode::ForbiddenExpression));
            }
            "SPC" | "TAB" => {
                return Err(self.err(ErrorCode::ForbiddenExpression));
            }
            _ => {}
        }
        if let Some(value) = call_pure_function(name, args.clone())? {
            return Ok(value);
        }
        match name.to_ascii_uppercase().as_str() {
            "RND" if args.is_empty() => Ok(Value::number(self.rng.next_f64())),
            "TIME" if args.is_empty() => Ok(Value::number(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64(),
            )),
            "REMAIN" if args.len() == 1 => Ok(Value::number(self.remain_value())),
            "ERR" if args.is_empty() => Ok(Value::number(
                self.last_error
                    .as_ref()
                    .map_or(0.0, |state| state.number as f64),
            )),
            "VERSION$" if args.is_empty() => Ok(Value::string(AVL_BASIC_LANGUAGE_VERSION)),
            "ERL" if args.is_empty() => Ok(Value::number(
                self.last_error
                    .as_ref()
                    .map_or(0.0, |state| state.line as f64),
            )),
            "INKEY$" if args.is_empty() => Ok(Value::string(self.read_inkey())),
            "KEYDOWN" if args.len() == 1 => {
                let code = args[0].as_number()?;
                if code.fract() != 0.0 || !(0.0..=255.0).contains(&code) {
                    return Err(self.err(ErrorCode::InvalidArgument));
                }
                Ok(Value::basic_bool(self.key_down(code as u8)))
            }
            "AMAXCOL" | "AMAXROW" | "AMINCOL" | "AMINROW" | "CNORMCOL" | "MAXABCOL"
            | "MAXABROW" | "RNORMROW"
                if args.is_empty() =>
            {
                Ok(Value::number(
                    self.numeric_variables
                        .get(&name.to_ascii_uppercase())
                        .copied()
                        .unwrap_or(0.0),
                ))
            }
            "WIDTH" if args.is_empty() => Ok(Value::number(self.graphics.width as f64)),
            "HEIGHT" if args.is_empty() => Ok(Value::number(self.graphics.height as f64)),
            "XPOS" if args.is_empty() => Ok(Value::number(self.graphics.xpos())),
            "YPOS" if args.is_empty() => Ok(Value::number(self.graphics.ypos())),
            "HPOS" if args.is_empty() => Ok(Value::number(self.graphics.hpos() as f64)),
            "VPOS" if args.is_empty() => Ok(Value::number(self.graphics.vpos() as f64)),
            "MOUSEX" if args.is_empty() => Ok(Value::number(self.mouse_state.x as f64)),
            "MOUSEY" if args.is_empty() => Ok(Value::number(self.mouse_state.y as f64)),
            "MOUSELEFT" if args.is_empty() => Ok(Value::number(if self.mouse_state.left {
                -1.0
            } else {
                0.0
            })),
            "MOUSERIGHT" if args.is_empty() => Ok(Value::number(if self.mouse_state.right {
                -1.0
            } else {
                0.0
            })),
            "MOUSEEVENT$" if args.is_empty() => Ok(Value::string(self.mouse_state.event.clone())),
            "SCREEN$" if args.is_empty() => Ok(Value::string(self.graphics.capture_screen())),
            "SPRITE$" if args.len() == 4 => Ok(Value::string(self.graphics.capture_sprite(
                args[0].as_number()?,
                args[1].as_number()?,
                args[2].as_number()?,
                args[3].as_number()?,
            ))),
            "TEST" if args.len() == 2 => Ok(Value::number(
                self.graphics
                    .test(args[0].as_number()?, args[1].as_number()?) as f64,
            )),
            "HIT" if args.is_empty() => Ok(Value::number(self.graphics.hit() as f64)),
            "HITCOLOR" if args.is_empty() => Ok(Value::number(self.graphics.hitcolor() as f64)),
            "HITSPRITE" if args.is_empty() => Ok(Value::number(self.graphics.hitsprite() as f64)),
            "HITID" if args.is_empty() => Ok(Value::number(self.graphics.hitid() as f64)),
            "RGB" => match args.as_slice() {
                [Value::Number(r), Value::Number(g), Value::Number(b)] => Ok(Value::number(
                    rgb_number(*r as i32, *g as i32, *b as i32)? as f64,
                )),
                [Value::Str(s)] => {
                    let (r, g, b) = parse_rgb_string(s)?;
                    Ok(Value::number(rgb_number(r, g, b)? as f64))
                }
                [Value::Number(n)] if *n as i32 == 31 => Ok(Value::number(16_777_200.0)),
                [Value::Number(n)] => Ok(Value::number(*n)),
                _ => Err(self.err(ErrorCode::ArgumentMismatch)),
            },
            "RGB$" => match args.as_slice() {
                [Value::Number(r), Value::Number(g), Value::Number(b)] => Ok(Value::string(
                    format!("{},{},{}", *r as i32, *g as i32, *b as i32),
                )),
                [Value::Str(s)] => {
                    let (r, g, b) = parse_rgb_string(s)?;
                    Ok(Value::string(format!("{r},{g},{b}")))
                }
                [Value::Number(n)] if *n as i32 == 32 || *n as i32 == 31 => {
                    Ok(Value::string(format!("0,0,{}", *n as i32)))
                }
                [Value::Number(n)] => {
                    let n = *n as i32;
                    Ok(Value::string(format!(
                        "{},{},{}",
                        (n >> 16) & 255,
                        (n >> 8) & 255,
                        n & 255
                    )))
                }
                _ => Err(self.err(ErrorCode::ArgumentMismatch)),
            },
            name if name.starts_with("FN") => self.call_user_function(name, args),
            _ => Err(self.err(ErrorCode::Undefined)),
        }
    }

    fn with_string_variable<R, F: FnOnce(&str) -> R>(&mut self, name: &str, f: F) -> Option<R> {
        let name = simple_string_variable_name(name)?;
        let text = self
            .string_variables
            .get(&name)
            .map(String::as_str)
            .unwrap_or("");
        Some(f(text))
    }

    fn string_variable_slice(
        &mut self,
        name: &str,
        start: usize,
        count: Option<usize>,
    ) -> Option<BasicResult<String>> {
        let name = simple_string_variable_name(name)?;
        let Some(text) = self.string_variables.get(&name) else {
            return Some(Ok(String::new()));
        };
        Some(Ok(basic_string_slice(text, start, count)))
    }
}

impl Interpreter {
    fn call_array_bound_function(&mut self, name: &str, args: Vec<Value>) -> BasicResult<Value> {
        if args.is_empty() || args.len() > 2 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let array_name = args[0].clone().into_string()?.to_ascii_uppercase();
        let Some(array) = self.arrays.get(&array_name) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        let round_dimension = matches!(name.to_ascii_uppercase().as_str(), "LBND" | "UBND");
        let dimension = if args.len() == 2 {
            let raw = args[1].as_number()?;
            if round_dimension {
                round_half_away(raw, 0) as usize
            } else {
                raw as usize
            }
        } else {
            1
        };
        let max_dimension = if round_dimension {
            array.dims.len().min(2)
        } else {
            array.dims.len()
        };
        if dimension == 0 || dimension > max_dimension {
            return Err(self.err(ErrorCode::IndexOutOfRange));
        }
        let upper = name.to_ascii_uppercase();
        let value = if upper == "LBOUND" || upper == "LBND" {
            self.mat_base as f64
        } else {
            array.dims[dimension - 1] as f64
        };
        Ok(Value::number(value))
    }

    fn call_det_function(&mut self, args: Vec<Value>) -> BasicResult<Value> {
        if args.len() != 1 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let array_name = args[0].clone().into_string()?.to_ascii_uppercase();
        let Some(array) = self.arrays.get(&array_name) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        let matrix = self.array_to_numeric_matrix(array)?;
        Ok(Value::number(self.determinant_numeric_matrix(matrix)?))
    }

    fn call_mat_stat_function(&mut self, name: &str, args: Vec<Value>) -> BasicResult<Value> {
        let upper = name.to_ascii_uppercase();
        if upper == "DOT" {
            if args.len() != 2 {
                return Err(self.err(ErrorCode::ArgumentMismatch));
            }
            let left_name = args[0].clone().into_string()?.to_ascii_uppercase();
            let right_name = args[1].clone().into_string()?.to_ascii_uppercase();
            let left = self
                .arrays
                .get(&left_name)
                .ok_or_else(|| self.err(ErrorCode::Undefined))?;
            let right = self
                .arrays
                .get(&right_name)
                .ok_or_else(|| self.err(ErrorCode::Undefined))?;
            let left_values = self.numeric_vector_values(left)?;
            let right_values = self.numeric_vector_values(right)?;
            if left_values.len() != right_values.len() {
                return Err(self.err(ErrorCode::InvalidDimensions));
            }
            let total = left_values
                .into_iter()
                .zip(right_values)
                .map(|(a, b)| a * b)
                .sum::<f64>();
            return Ok(Value::number(total));
        }

        if args.len() != 1 {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let array_name = args[0].clone().into_string()?.to_ascii_uppercase();
        let Some(array) = self.arrays.get(&array_name) else {
            return Err(self.err(ErrorCode::Undefined));
        };
        let stats = self.analyze_numeric_array(array)?;
        let value = match upper.as_str() {
            "ABSUM" => stats.abs_sum,
            "AMAX" => {
                self.set_mat_stat_context("AMAX", stats.max_pos);
                stats.max
            }
            "AMIN" => {
                self.set_mat_stat_context("AMIN", stats.min_pos);
                stats.min
            }
            "CNORM" => {
                self.numeric_variables.insert(
                    "CNORMCOL".to_string(),
                    stats.col_norm_col.unwrap_or(0) as f64,
                );
                stats.col_norm
            }
            "FNORM" => stats.fnorm,
            "MAXAB" => {
                self.set_mat_stat_context("MAXAB", stats.max_abs_pos);
                stats.max_abs
            }
            "RNORM" => {
                self.numeric_variables.insert(
                    "RNORMROW".to_string(),
                    stats.row_norm_row.unwrap_or(0) as f64,
                );
                stats.row_norm
            }
            "SUM" => stats.sum,
            _ => return Err(self.err(ErrorCode::Undefined)),
        };
        Ok(Value::number(value))
    }

    fn set_mat_stat_context(&mut self, prefix: &str, position: Option<(i32, i32)>) {
        let (row, col) = position.unwrap_or((0, 0));
        self.numeric_variables
            .insert(format!("{prefix}ROW"), row as f64);
        self.numeric_variables
            .insert(format!("{prefix}COL"), col as f64);
    }

    fn numeric_vector_values(&self, array: &ArrayValue) -> BasicResult<Vec<f64>> {
        let matrix = self.array_to_numeric_matrix(array)?;
        if matrix.cols != 1 {
            return Err(self.err(ErrorCode::InvalidDimensions));
        }
        Ok(matrix.data)
    }

    fn analyze_numeric_array(&self, array: &ArrayValue) -> BasicResult<MatStats> {
        let matrix = self.array_to_numeric_matrix(array)?;
        let lower = self.mat_base.max(0) as i32;
        let mut stats = MatStats::default();
        if matrix.rows == 0 || matrix.cols == 0 {
            return Ok(stats);
        }
        stats.max = f64::NEG_INFINITY;
        stats.min = f64::INFINITY;
        let mut row_sums = vec![0.0; matrix.rows];
        let mut col_sums = vec![0.0; matrix.cols];
        for r in 0..matrix.rows {
            for c in 0..matrix.cols {
                let value = matrix.data[r * matrix.cols + c];
                let abs = value.abs();
                let row_index = lower + r as i32;
                let col_index = lower + c as i32;
                stats.sum += value;
                stats.abs_sum += abs;
                stats.fnorm += value * value;
                row_sums[r] += abs;
                col_sums[c] += abs;
                if stats.max_pos.is_none() || value > stats.max {
                    stats.max = value;
                    stats.max_pos = Some((row_index, col_index));
                }
                if stats.min_pos.is_none() || value < stats.min {
                    stats.min = value;
                    stats.min_pos = Some((row_index, col_index));
                }
                if stats.max_abs_pos.is_none() || abs > stats.max_abs {
                    stats.max_abs = abs;
                    stats.max_abs_pos = Some((row_index, col_index));
                }
            }
        }
        stats.fnorm = stats.fnorm.sqrt();
        for (idx, value) in col_sums.into_iter().enumerate() {
            if stats.col_norm_col.is_none() || value > stats.col_norm {
                stats.col_norm = value;
                stats.col_norm_col = Some(lower + idx as i32);
            }
        }
        for (idx, value) in row_sums.into_iter().enumerate() {
            if stats.row_norm_row.is_none() || value > stats.row_norm {
                stats.row_norm = value;
                stats.row_norm_row = Some(lower + idx as i32);
            }
        }
        Ok(stats)
    }

    fn call_user_function(&mut self, name: &str, args: Vec<Value>) -> BasicResult<Value> {
        let Some(fun) = self.functions.get(name).cloned() else {
            return Err(self.err(ErrorCode::Undefined));
        };
        let name = name.to_ascii_uppercase();
        if args.is_empty() {
            if let Some(value) = self.active_function_variable(&name) {
                return Ok(value);
            }
        }
        if self
            .function_call_stack
            .iter()
            .any(|active| active == &name)
        {
            return Err(self.err(ErrorCode::FunctionForbidden));
        }
        match fun {
            UserFunction::Single { params, expr } => {
                if params.len() != args.len() {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                if args.iter().any(|arg| matches!(arg, Value::ArrayRef(_))) {
                    return Err(self.err(ErrorCode::TypeMismatch));
                }
                let saved_numeric = self.numeric_variables.clone();
                let saved_string = self.string_variables.clone();
                let saved_arrays = self.arrays.clone();
                self.function_call_stack.push(name.clone());
                let bind_result = self.bind_function_args(&params, args);
                let result = if bind_result.is_ok() {
                    self.eval_value(&expr)
                } else {
                    Err(bind_result.err().unwrap())
                };
                let return_array = match &result {
                    Ok(Value::ArrayRef(source)) => self.arrays.get(source).cloned(),
                    _ => None,
                };
                self.function_call_stack.pop();
                self.numeric_variables = saved_numeric;
                self.string_variables = saved_string;
                self.arrays = saved_arrays;
                if return_array.is_some() {
                    return Err(self.err(ErrorCode::TypeMismatch));
                }
                result
            }
            UserFunction::Multi {
                params,
                local_specs,
                start,
                ..
            } => {
                if params.len() != args.len() {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                self.call_multiline_function(name, params, local_specs, args, start)
            }
        }
    }

    fn bind_function_args(&mut self, params: &[String], args: Vec<Value>) -> BasicResult<()> {
        for (param, value) in params.iter().zip(args.into_iter()) {
            match value {
                Value::Number(n) => {
                    if param.ends_with('$') {
                        return Err(self.err(ErrorCode::TypeMismatch));
                    }
                    self.numeric_variables.insert(param.clone(), n);
                }
                Value::Str(s) => {
                    if !param.ends_with('$') {
                        return Err(self.err(ErrorCode::TypeMismatch));
                    }
                    self.string_variables.insert(param.clone(), s);
                }
                Value::ArrayRef(source) => {
                    let Some(array) = self.arrays.get(&source).cloned() else {
                        return Err(self.err(ErrorCode::Undefined));
                    };
                    if param.ends_with('$') != array.is_string() {
                        return Err(self.err(ErrorCode::TypeMismatch));
                    }
                    self.arrays.insert(param.clone(), array);
                }
            }
        }
        Ok(())
    }

    fn bind_local_specs(
        &mut self,
        local_specs: &[LocalSpec],
        saved_numeric: &mut HashMap<String, Option<f64>>,
        saved_string: &mut HashMap<String, Option<String>>,
        saved_arrays: &mut HashMap<String, Option<ArrayValue>>,
    ) -> BasicResult<()> {
        for spec in local_specs {
            match spec {
                LocalSpec::Scalar(name) => {
                    if name.ends_with('$') {
                        saved_string
                            .entry(name.clone())
                            .or_insert_with(|| self.string_variables.get(name).cloned());
                        self.string_variables.insert(name.clone(), String::new());
                    } else {
                        saved_numeric
                            .entry(name.clone())
                            .or_insert_with(|| self.numeric_variables.get(name).copied());
                        self.numeric_variables.insert(name.clone(), 0.0);
                    }
                }
                LocalSpec::Array { name, dims } => {
                    let mut evaluated = Vec::with_capacity(dims.len());
                    for dim in dims {
                        let value = self.eval_number(dim)?;
                        if value < 0.0 {
                            return Err(self.err(ErrorCode::InvalidValue));
                        }
                        evaluated.push(value as usize);
                    }
                    saved_arrays
                        .entry(name.clone())
                        .or_insert_with(|| self.arrays.get(name).cloned());
                    self.arrays
                        .insert(name.clone(), ArrayValue::new(name, evaluated));
                }
            }
        }
        Ok(())
    }

    fn execute_call(&mut self, args: &str) -> BasicResult<()> {
        if self.inside_multiline_function() {
            return Err(self.err(ErrorCode::FunctionForbidden));
        }
        let (name, arg_exprs) = parse_call_statement(args)?;
        let mut values = Vec::with_capacity(arg_exprs.len());
        for expr in arg_exprs {
            values.push(self.eval_sub_call_argument(&expr)?);
        }
        self.call_multiline_sub(name, values)
    }

    fn eval_sub_call_argument(&mut self, expr: &str) -> BasicResult<Value> {
        let trimmed = expr.trim();
        let upper = trimmed.to_ascii_uppercase();
        if is_basic_identifier(&upper)
            && self.arrays.contains_key(&upper)
            && !self.numeric_variables.contains_key(&upper)
            && !self.string_variables.contains_key(&upper)
        {
            return Ok(Value::ArrayRef(upper));
        }
        self.eval_value(trimmed)
    }

    fn call_multiline_sub(&mut self, name: String, args: Vec<Value>) -> BasicResult<()> {
        let Some(sub) = self.subs.get(&name).cloned() else {
            return Err(self.err(ErrorCode::Undefined));
        };
        if self.sub_call_stack.iter().any(|active| active == &name) {
            return Err(self.err(ErrorCode::SubEndWithoutDef));
        }
        if sub.params.len() != args.len() {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }

        let mut saved_numeric = HashMap::new();
        let mut saved_string = HashMap::new();
        let mut saved_arrays = HashMap::new();
        let mut array_copybacks: Vec<(String, String)> = Vec::new();

        if name.ends_with('$') {
            saved_string.insert(name.clone(), self.string_variables.get(&name).cloned());
        } else {
            saved_numeric.insert(name.clone(), self.numeric_variables.get(&name).copied());
        }

        for (param, value) in sub.params.iter().zip(args.into_iter()) {
            match value {
                Value::Number(n) => {
                    if param.ends_with('$') {
                        restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
                        restore_string_bindings(&mut self.string_variables, saved_string);
                        restore_array_bindings(&mut self.arrays, saved_arrays);
                        return Err(self.err(ErrorCode::TypeMismatch));
                    }
                    saved_numeric
                        .entry(param.clone())
                        .or_insert_with(|| self.numeric_variables.get(param).copied());
                    self.numeric_variables.insert(param.clone(), n);
                }
                Value::Str(s) => {
                    if !param.ends_with('$') {
                        restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
                        restore_string_bindings(&mut self.string_variables, saved_string);
                        restore_array_bindings(&mut self.arrays, saved_arrays);
                        return Err(self.err(ErrorCode::TypeMismatch));
                    }
                    saved_string
                        .entry(param.clone())
                        .or_insert_with(|| self.string_variables.get(param).cloned());
                    self.string_variables.insert(param.clone(), s);
                }
                Value::ArrayRef(source) => {
                    let Some(array) = self.arrays.get(&source).cloned() else {
                        restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
                        restore_string_bindings(&mut self.string_variables, saved_string);
                        restore_array_bindings(&mut self.arrays, saved_arrays);
                        return Err(self.err(ErrorCode::Undefined));
                    };
                    if param.ends_with('$') != array.is_string() {
                        restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
                        restore_string_bindings(&mut self.string_variables, saved_string);
                        restore_array_bindings(&mut self.arrays, saved_arrays);
                        return Err(self.err(ErrorCode::TypeMismatch));
                    }
                    saved_numeric
                        .entry(param.clone())
                        .or_insert_with(|| self.numeric_variables.get(param).copied());
                    saved_string
                        .entry(param.clone())
                        .or_insert_with(|| self.string_variables.get(param).cloned());
                    self.numeric_variables.remove(param);
                    self.string_variables.remove(param);
                    saved_arrays
                        .entry(param.clone())
                        .or_insert_with(|| self.arrays.get(param).cloned());
                    self.arrays.insert(param.clone(), array);
                    array_copybacks.push((param.clone(), source));
                }
            }
        }

        if let Err(err) = self.bind_local_specs(
            &sub.local_specs,
            &mut saved_numeric,
            &mut saved_string,
            &mut saved_arrays,
        ) {
            restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
            restore_string_bindings(&mut self.string_variables, saved_string);
            restore_array_bindings(&mut self.arrays, saved_arrays);
            return Err(err);
        }

        let saved_current_line = self.current_line;
        let saved_for_len = self.for_stack.len();
        let saved_while_len = self.while_stack.len();
        let saved_gosub_len = self.gosub_stack.len();
        let saved_if_stack = std::mem::take(&mut self.if_stack);
        let saved_pending_if = self.pending_if_branch.take();
        let previous_sub_return = self.sub_return_requested;
        self.sub_return_requested = false;
        self.sub_call_stack.push(name.clone());
        self.active_subs.push(ActiveSubFrame { name });

        let run_result = self.run_from(sub.start);
        self.active_subs.pop();
        self.sub_call_stack.pop();

        for (param, source) in &array_copybacks {
            if let Some(array) = self.arrays.get(param).cloned() {
                self.arrays.insert(source.clone(), array);
            }
        }

        self.for_stack.truncate(saved_for_len);
        self.while_stack.truncate(saved_while_len);
        self.gosub_stack.truncate(saved_gosub_len);
        self.if_stack = saved_if_stack;
        self.pending_if_branch = saved_pending_if;
        self.current_line = saved_current_line;
        self.sub_return_requested = previous_sub_return;
        restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
        restore_string_bindings(&mut self.string_variables, saved_string);
        restore_array_bindings(&mut self.arrays, saved_arrays);

        run_result?;
        Ok(())
    }

    fn call_multiline_function(
        &mut self,
        name: String,
        params: Vec<String>,
        local_specs: Vec<LocalSpec>,
        args: Vec<Value>,
        start: Cursor,
    ) -> BasicResult<Value> {
        let mut saved_numeric = HashMap::new();
        let mut saved_string = HashMap::new();
        let mut saved_arrays = HashMap::new();
        for param in &params {
            if param.ends_with('$') {
                saved_string.insert(param.clone(), self.string_variables.get(param).cloned());
            } else {
                saved_numeric.insert(param.clone(), self.numeric_variables.get(param).copied());
            }
            saved_arrays.insert(param.clone(), self.arrays.get(param).cloned());
        }
        if name.ends_with('$') {
            saved_string.insert(name.clone(), self.string_variables.get(&name).cloned());
        } else {
            saved_numeric.insert(name.clone(), self.numeric_variables.get(&name).copied());
        }
        saved_arrays.insert(name.clone(), self.arrays.get(&name).cloned());

        self.function_call_stack.push(name.clone());
        if let Err(err) = self.bind_function_args(&params, args) {
            self.function_call_stack.pop();
            restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
            restore_string_bindings(&mut self.string_variables, saved_string);
            restore_array_bindings(&mut self.arrays, saved_arrays);
            return Err(err);
        }
        if let Err(err) = self.bind_local_specs(
            &local_specs,
            &mut saved_numeric,
            &mut saved_string,
            &mut saved_arrays,
        ) {
            self.function_call_stack.pop();
            restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
            restore_string_bindings(&mut self.string_variables, saved_string);
            restore_array_bindings(&mut self.arrays, saved_arrays);
            return Err(err);
        }

        let saved_current_line = self.current_line;
        let saved_for_len = self.for_stack.len();
        let saved_while_len = self.while_stack.len();
        let saved_gosub_len = self.gosub_stack.len();
        let saved_if_stack = std::mem::take(&mut self.if_stack);
        let saved_pending_if = self.pending_if_branch.take();
        let previous_function_return = self.function_return_requested;
        self.function_return_requested = false;
        self.active_functions.push(ActiveFunctionFrame {
            name: name.clone(),
            return_value: None,
        });

        let run_result = self.run_from(start);
        let active = self.active_functions.pop();
        let mut return_value = active.and_then(|frame| frame.return_value);
        let return_array = if let Some(Value::ArrayRef(source)) = &return_value {
            self.arrays.get(source).cloned()
        } else {
            None
        };
        self.function_call_stack.pop();
        self.for_stack.truncate(saved_for_len);
        self.while_stack.truncate(saved_while_len);
        self.gosub_stack.truncate(saved_gosub_len);
        self.if_stack = saved_if_stack;
        self.pending_if_branch = saved_pending_if;
        self.current_line = saved_current_line;
        self.function_return_requested = previous_function_return;

        restore_numeric_bindings(&mut self.numeric_variables, saved_numeric);
        restore_string_bindings(&mut self.string_variables, saved_string);
        restore_array_bindings(&mut self.arrays, saved_arrays);
        if let Some(array) = return_array {
            self.arrays.insert(name.clone(), array);
            return_value = Some(Value::ArrayRef(name.clone()));
        }

        run_result?;
        Ok(return_value.unwrap_or_else(|| Value::default_for_name(&name)))
    }
}

#[cfg(test)]
mod interpreter_tests {
    use super::*;

    #[test]
    fn handled_mouse_event_is_consumed_for_pause_and_not_replayed_at_prompt() {
        let mut interp = Interpreter::new();
        interp
            .program
            .load_text(
                r#"100 A=A+1
110 RETURN"#,
            )
            .unwrap();
        interp.rebuild_command_cache();
        interp.mouse_handlers.insert("LEFTDOWN".to_string(), 100);
        interp.mouse_state = MouseSnapshot {
            x: 10,
            y: 20,
            left: true,
            right: false,
            event: "LEFTDOWN".to_string(),
        };
        interp.current_line = Some(20);
        interp.run_depth = 1;

        interp.process_mouse_event().unwrap();

        assert_eq!(interp.numeric_variables.get("A").copied(), Some(1.0));
        assert_eq!(interp.current_line, Some(20));
        assert!(interp.mouse_event_consumed);

        interp.process_mouse_event().unwrap();
        assert_eq!(interp.numeric_variables.get("A").copied(), Some(1.0));

        interp.run_depth = 0;
        interp.mouse_event_consumed = false;
        interp.process_mouse_event().unwrap();
        assert_eq!(interp.numeric_variables.get("A").copied(), Some(1.0));
    }

    #[test]
    fn closing_stale_graphics_window_does_not_interrupt_unattached_run() {
        let mut interp = Interpreter::new();
        interp.current_line = Some(20);
        interp.graphics_window_used_this_run = false;

        interp.mark_graphics_window_user_closed();

        assert!(!interp.test_interrupt_requested);
    }

    #[test]
    fn closing_graphics_window_interrupts_run_after_graphics_use() {
        let mut interp = Interpreter::new();
        interp.current_line = Some(20);
        interp.graphics_window_used_this_run = true;

        interp.mark_graphics_window_user_closed();

        assert!(interp.test_interrupt_requested);
    }

    #[test]
    fn graphics_keyboard_is_used_only_after_current_run_uses_window() {
        let mut interp = Interpreter::new();
        interp.current_line = Some(20);
        interp.graphics_window_used_this_run = false;
        assert!(!interp.current_run_uses_graphics_window());

        interp.graphics_window_used_this_run = true;
        assert!(interp.current_run_uses_graphics_window());

        interp.current_line = None;
        assert!(!interp.current_run_uses_graphics_window());
    }
}

fn graphics_window_enabled() -> bool {
    match std::env::var("AVL_BASIC_WINDOW") {
        Ok(value)
            if matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ) =>
        {
            true
        }
        Ok(value)
            if matches!(
                value.to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            ) =>
        {
            false
        }
        _ => io::stdout().is_terminal(),
    }
}

#[derive(Debug, Clone)]
struct SimpleRng {
    state: [u32; 624],
    index: usize,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        let key = if seed <= u32::MAX as u64 {
            vec![seed as u32]
        } else {
            vec![seed as u32, (seed >> 32) as u32]
        };
        Self::from_key(&key)
    }

    fn from_key(key: &[u32]) -> Self {
        let mut state = [0u32; 624];
        state[0] = 19650218;
        for i in 1..624 {
            state[i] = 1812433253u32
                .wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let mut i = 1usize;
        let mut j = 0usize;
        let mut k = 624usize.max(key.len());
        while k > 0 {
            state[i] = (state[i] ^ ((state[i - 1] ^ (state[i - 1] >> 30)).wrapping_mul(1664525)))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= 624 {
                state[0] = state[623];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
            k -= 1;
        }
        for _ in 1..624 {
            state[i] = (state[i]
                ^ ((state[i - 1] ^ (state[i - 1] >> 30)).wrapping_mul(1566083941)))
            .wrapping_sub(i as u32);
            i += 1;
            if i >= 624 {
                state[0] = state[623];
                i = 1;
            }
        }
        state[0] = 0x8000_0000;
        Self { state, index: 624 }
    }

    fn next_f64(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as u64;
        let b = (self.next_u32() >> 6) as u64;
        ((a << 26) + b) as f64 / ((1u64 << 53) as f64)
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= 624 {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9D2C5680;
        y ^= (y << 15) & 0xEFC60000;
        y ^= y >> 18;
        y
    }

    fn twist(&mut self) {
        for i in 0..624 {
            let x = (self.state[i] & 0x8000_0000) + (self.state[(i + 1) % 624] & 0x7fff_ffff);
            let mut x_a = x >> 1;
            if x & 1 != 0 {
                x_a ^= 0x9908_B0DF;
            }
            self.state[i] = self.state[(i + 397) % 624] ^ x_a;
        }
        self.index = 0;
    }
}

fn starts_with_line_number(line: &str) -> bool {
    let mut chars = line.chars().peekable();
    let mut saw_digit = false;
    while chars.peek().is_some_and(|ch| ch.is_ascii_digit()) {
        saw_digit = true;
        chars.next();
    }
    saw_digit && chars.peek().map_or(true, |ch| ch.is_whitespace())
}

fn is_assignment(command: &str) -> bool {
    find_assignment_equal(command).is_some()
}

fn first_word_is(command: &str, word: &str) -> bool {
    command
        .trim_start()
        .split_whitespace()
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case(word))
}

fn immediate_arg<'a>(command: &'a str, upper_command: &str, word: &str) -> Option<&'a str> {
    let upper_word = word.to_ascii_uppercase();
    if upper_command == upper_word {
        return Some("");
    }
    let rest = upper_command.strip_prefix(&upper_word)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(command[word.len()..].trim())
}

fn is_non_immediate_command(first: &str) -> bool {
    matches!(
        first,
        "DATA" | "RETURN" | "GOSUB" | "STOP" | "RESUME" | "ON" | "ERROR"
    )
}

fn is_immediate_only_command(first: &str, upper_command: &str) -> bool {
    matches!(
        first,
        "NEW"
            | "LIST"
            | "RUN"
            | "SAVE"
            | "LOAD"
            | "FILES"
            | "CAT"
            | "CD"
            | "CONT"
            | "RENUM"
            | "DEBUG"
            | "EDIT"
            | "DELETE"
            | "SYSTEM"
            | "QUIT"
    ) || (first == "EXIT"
        && !matches!(
            upper_command,
            "EXIT FOR" | "EXIT WHILE" | "EXIT FN" | "EXIT SUB"
        ))
}

fn parse_line_number_literal(source: &str) -> Option<i32> {
    let trimmed = source.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    trimmed.parse::<i32>().ok()
}

#[derive(Debug, Clone, Copy)]
enum LineRangeSpec {
    Single(i32),
    Range(Option<i32>, Option<i32>),
}

impl LineRangeSpec {
    fn bounds_for(self, lines: &[i32]) -> (i32, i32) {
        match self {
            LineRangeSpec::Single(line) => (line, line),
            LineRangeSpec::Range(start, end) => {
                let first = lines.first().copied().unwrap_or(0);
                let last = lines.last().copied().unwrap_or(0);
                (start.unwrap_or(first), end.unwrap_or(last))
            }
        }
    }
}

fn parse_line_range_spec(
    source: &str,
    single_invalid: ErrorCode,
    range_invalid: ErrorCode,
) -> BasicResult<LineRangeSpec> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Ok(LineRangeSpec::Range(None, None));
    }
    if let Some((start, end)) = trimmed.split_once('-') {
        let start = if start.trim().is_empty() {
            None
        } else {
            Some(parse_line_number_literal(start).ok_or_else(|| BasicError::new(range_invalid))?)
        };
        let end = if end.trim().is_empty() {
            None
        } else {
            Some(parse_line_number_literal(end).ok_or_else(|| BasicError::new(range_invalid))?)
        };
        if start.is_some() && end.is_some() && start > end {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        return Ok(LineRangeSpec::Range(start, end));
    }
    let line = parse_line_number_literal(trimmed).ok_or_else(|| BasicError::new(single_invalid))?;
    Ok(LineRangeSpec::Single(line))
}

fn parse_delete_range(source: &str) -> BasicResult<(i32, i32)> {
    let trimmed = source.trim();
    let Some((start, end)) = trimmed.split_once('-') else {
        let line = parse_line_number_literal(trimmed)
            .ok_or_else(|| BasicError::new(ErrorCode::InvalidLineNumber))?;
        return Ok((line, line));
    };
    let start = parse_line_number_literal(start)
        .ok_or_else(|| BasicError::new(ErrorCode::InvalidLineNumber))?;
    let end = parse_line_number_literal(end)
        .ok_or_else(|| BasicError::new(ErrorCode::InvalidLineNumber))?;
    Ok((start.min(end), start.max(end)))
}

fn parse_function_header(header: &str) -> BasicResult<(String, Vec<String>)> {
    let open = header.find('(');
    let (name, params) = if let Some(open) = open {
        let close = header
            .rfind(')')
            .ok_or_else(|| BasicError::new(ErrorCode::Syntax))?;
        let raw_params = header[open + 1..close].trim();
        let params = if raw_params.is_empty() {
            Vec::new()
        } else {
            split_arguments(raw_params)
                .into_iter()
                .map(|s| s.to_ascii_uppercase())
                .collect()
        };
        (header[..open].trim().to_ascii_uppercase(), params)
    } else {
        (header.trim().to_ascii_uppercase(), Vec::new())
    };
    if !is_basic_identifier(&name) || !name.starts_with("FN") {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    if params.iter().any(|param| !is_basic_identifier(param)) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    Ok((name, params))
}

fn parse_sub_header(header: &str) -> BasicResult<(String, Vec<String>)> {
    let open = header.find('(');
    let (name, params) = if let Some(open) = open {
        let close = header
            .rfind(')')
            .ok_or_else(|| BasicError::new(ErrorCode::Syntax))?;
        if !header[close + 1..].trim().is_empty() {
            return Err(BasicError::new(ErrorCode::Syntax));
        }
        let raw_params = header[open + 1..close].trim();
        let params = if raw_params.is_empty() {
            Vec::new()
        } else {
            split_arguments(raw_params)
                .into_iter()
                .map(|s| s.to_ascii_uppercase())
                .collect()
        };
        (header[..open].trim().to_ascii_uppercase(), params)
    } else {
        (header.trim().to_ascii_uppercase(), Vec::new())
    };
    if !is_basic_identifier(&name) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    let mut seen = Vec::new();
    for param in &params {
        if !is_basic_identifier(param)
            || param.eq_ignore_ascii_case(&name)
            || seen.iter().any(|existing: &String| existing == param)
        {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        seen.push(param.clone());
    }
    Ok((name, params))
}

fn axis_tics_token_is_scientific(token: &str) -> bool {
    let mut text = token.trim();
    if text.is_empty() {
        return false;
    }

    loop {
        let Some(inner) = text.strip_prefix('(').and_then(|s| s.strip_suffix(')')) else {
            break;
        };
        let mut depth = 0i32;
        let mut encloses_all = true;
        for (idx, ch) in text.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && idx != text.len() - 1 {
                        encloses_all = false;
                        break;
                    }
                }
                _ => {}
            }
            if depth < 0 {
                encloses_all = false;
                break;
            }
        }
        if !encloses_all || depth != 0 {
            break;
        }
        text = inner.trim();
        if text.is_empty() {
            return false;
        }
    }

    let normalized = text.replace('_', "");
    let mut chars = normalized.chars().peekable();
    if matches!(chars.peek().copied(), Some('+') | Some('-')) {
        chars.next();
    }

    let mut digits_before = 0usize;
    while matches!(chars.peek().copied(), Some(ch) if ch.is_ascii_digit()) {
        digits_before += 1;
        chars.next();
    }

    let mut digits_after = 0usize;
    if matches!(chars.peek().copied(), Some('.')) {
        chars.next();
        while matches!(chars.peek().copied(), Some(ch) if ch.is_ascii_digit()) {
            digits_after += 1;
            chars.next();
        }
    }

    if digits_before == 0 && digits_after == 0 {
        return false;
    }
    if !matches!(chars.next(), Some('E') | Some('e')) {
        return false;
    }
    if matches!(chars.peek().copied(), Some('+') | Some('-')) {
        chars.next();
    }

    let mut exponent_digits = 0usize;
    while matches!(chars.peek().copied(), Some(ch) if ch.is_ascii_digit()) {
        exponent_digits += 1;
        chars.next();
    }
    exponent_digits > 0 && chars.next().is_none()
}

fn simple_string_variable_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    (trimmed.ends_with('$') && is_basic_identifier(trimmed)).then(|| trimmed.to_ascii_uppercase())
}

fn basic_string_slice(text: &str, start: usize, count: Option<usize>) -> String {
    if text.is_ascii() {
        if start >= text.len() {
            return String::new();
        }
        let end = count
            .map_or(text.len(), |count| start.saturating_add(count))
            .min(text.len());
        return text[start..end].to_string();
    }

    let chars: Vec<_> = text.chars().collect();
    if start >= chars.len() {
        return String::new();
    }
    let end = count
        .map_or(chars.len(), |count| start.saturating_add(count))
        .min(chars.len());
    chars[start..end].iter().collect()
}

fn replace_mid_ascii(
    original: &mut String,
    start: usize,
    count: Option<usize>,
    replacement: &str,
) -> bool {
    if !original.is_ascii() || !replacement.is_ascii() {
        return false;
    }
    if start >= original.len() {
        return true;
    }
    let max_count = original.len() - start;
    let count = count.unwrap_or(max_count).min(max_count);
    let replacement_len = replacement.len().min(count);
    if replacement_len == 0 {
        return true;
    }
    original.replace_range(
        start..start + replacement_len,
        &replacement[..replacement_len],
    );
    true
}

fn replace_mid_general(
    original: &mut String,
    start: usize,
    count: Option<usize>,
    replacement: &str,
) {
    let mut chars: Vec<char> = original.chars().collect();
    if start >= chars.len() {
        return;
    }
    let max_count = chars.len() - start;
    let count = count.unwrap_or(max_count).min(max_count);
    if count == 0 {
        return;
    }
    for (offset, ch) in replacement.chars().take(count).enumerate() {
        chars[start + offset] = ch;
    }
    *original = chars.into_iter().collect();
}

fn parse_call_statement(args: &str) -> BasicResult<(String, Vec<String>)> {
    let text = args.trim();
    if text.is_empty() {
        return Err(BasicError::new(ErrorCode::Syntax));
    }
    let mut end = 0usize;
    for (idx, ch) in text.char_indices() {
        if idx == 0 {
            if !ch.is_ascii_alphabetic() && ch != '_' {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
        } else if !is_ident_char_for_parse(ch) {
            break;
        }
        end = idx + ch.len_utf8();
    }
    let name = text[..end].to_ascii_uppercase();
    if !is_basic_identifier(&name) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    let rest = text[end..].trim();
    if rest.is_empty() {
        return Ok((name, Vec::new()));
    }
    if !rest.starts_with('(') || !rest.ends_with(')') {
        return Err(BasicError::new(ErrorCode::Syntax));
    }
    let body = rest[1..rest.len() - 1].trim();
    let args = if body.is_empty() {
        Vec::new()
    } else {
        split_arguments(body)
    };
    Ok((name, args))
}

fn is_ident_char_for_parse(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn parse_local_specs(args: &str) -> BasicResult<Vec<LocalSpec>> {
    if args.trim().is_empty() {
        return Err(BasicError::new(ErrorCode::Syntax));
    }
    split_arguments(args)
        .into_iter()
        .map(|spec| {
            let spec = spec.trim().to_string();
            if spec.is_empty() {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            if let Some(open) = spec.find('(') {
                let close = spec
                    .rfind(')')
                    .ok_or_else(|| BasicError::new(ErrorCode::Syntax))?;
                if !spec[close + 1..].trim().is_empty() {
                    return Err(BasicError::new(ErrorCode::Syntax));
                }
                let name = spec[..open].trim().to_ascii_uppercase();
                if !is_basic_identifier(&name) {
                    return Err(BasicError::new(ErrorCode::InvalidArgument));
                }
                let dims = split_arguments(&spec[open + 1..close]);
                if dims.is_empty() || dims.iter().any(|dim| dim.trim().is_empty()) {
                    return Err(BasicError::new(ErrorCode::UndefinedIndex));
                }
                Ok(LocalSpec::Array { name, dims })
            } else {
                let name = spec.to_ascii_uppercase();
                if !is_basic_identifier(&name) {
                    return Err(BasicError::new(ErrorCode::InvalidArgument));
                }
                Ok(LocalSpec::Scalar(name))
            }
        })
        .collect()
}

fn local_spec_name(spec: &LocalSpec) -> &str {
    match spec {
        LocalSpec::Scalar(name) => name,
        LocalSpec::Array { name, .. } => name,
    }
}

fn assignment_target_is_string(lhs: &str) -> bool {
    lhs.trim()
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .ends_with('$')
}

fn numbered_line_code(source: &str) -> Option<&str> {
    let text = source.trim_start();
    let digit_end = text
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()?;
    let rest = &text[digit_end..];
    if rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace) {
        Some(rest)
    } else {
        None
    }
}

fn record_identifier_case_forms(
    source: &str,
    cases: &mut HashMap<String, String>,
    overwrite: bool,
) {
    for command in split_commands(source) {
        let trimmed = command.trim_start();
        let first = trimmed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if matches!(first.as_str(), "DATA" | "REM") {
            continue;
        }

        let chars: Vec<char> = command.chars().collect();
        let mut i = 0usize;
        let mut in_string = false;
        let mut after_def = false;
        let mut skip_callable_name = false;
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
            if ch.is_ascii_alphabetic() || ch == '_' {
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '$')
                {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                let upper = ident.to_ascii_uppercase();

                if skip_callable_name {
                    skip_callable_name = false;
                    after_def = false;
                    continue;
                }
                if upper == "DEF" {
                    after_def = true;
                    continue;
                }
                if upper == "CALL" {
                    skip_callable_name = true;
                    after_def = false;
                    continue;
                }
                if after_def {
                    after_def = false;
                    if upper == "SUB" {
                        skip_callable_name = true;
                    }
                    continue;
                }
                if upper.starts_with("FN") || console::is_known_basic_word(&upper) {
                    continue;
                }
                if is_basic_identifier(&ident) {
                    if overwrite {
                        cases.insert(upper, ident);
                    } else {
                        cases.entry(upper).or_insert(ident);
                    }
                }
                continue;
            }
            if !ch.is_whitespace() {
                after_def = false;
            }
            i += 1;
        }
    }
}

fn assignment_target_is_reserved_function(lhs: &str) -> bool {
    let name = lhs
        .trim()
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    is_reserved_matrix_function_name(&name)
}

fn is_reserved_matrix_function_name(name: &str) -> bool {
    matches!(
        name,
        "ABSUM"
            | "AMAX"
            | "AMAXCOL"
            | "AMAXROW"
            | "AMIN"
            | "AMINCOL"
            | "AMINROW"
            | "CNORM"
            | "CNORMCOL"
            | "DOT"
            | "FNORM"
            | "LBND"
            | "MAXAB"
            | "MAXABCOL"
            | "MAXABROW"
            | "RNORM"
            | "RNORMROW"
            | "SUM"
            | "UBND"
    )
}

fn assignment_identifier_case(lhs: &str) -> Option<(String, String)> {
    let trimmed = lhs.trim();
    let base = trimmed
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .strip_prefix("MID$")
        .unwrap_or_else(|| trimmed.split('(').next().unwrap_or("").trim());
    if is_basic_identifier(base) {
        Some((base.to_ascii_uppercase(), base.to_string()))
    } else {
        None
    }
}

fn is_basic_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

fn apply_identifier_case(source: &str, cases: &HashMap<String, String>) -> String {
    if cases.is_empty() {
        return source.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let mut chars = source.char_indices().peekable();
    let mut in_string = false;
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            continue;
        }
        if !in_string && ch == '\'' {
            out.push_str(&source[idx..]);
            break;
        }
        if !in_string && (ch.is_ascii_alphabetic() || ch == '_') {
            let start = idx;
            let mut end = idx + ch.len_utf8();
            while let Some((next_idx, next_ch)) = chars.peek().copied() {
                if next_ch.is_ascii_alphanumeric() || next_ch == '_' || next_ch == '$' {
                    chars.next();
                    end = next_idx + next_ch.len_utf8();
                } else {
                    break;
                }
            }
            let ident = &source[start..end];
            if ident.eq_ignore_ascii_case("REM") && identifier_boundary(source, start, end) {
                out.push_str(ident);
                out.push_str(&source[end..]);
                break;
            } else if let Some(display) = cases.get(&ident.to_ascii_uppercase()) {
                out.push_str(display);
            } else {
                out.push_str(ident);
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn identifier_boundary(source: &str, start: usize, end: usize) -> bool {
    let before = source[..start]
        .chars()
        .next_back()
        .map_or(true, |ch| !is_basic_identifier_char(ch));
    let after = source[end..]
        .chars()
        .next()
        .map_or(true, |ch| !is_basic_identifier_char(ch));
    before && after
}

fn is_basic_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn is_mouse_event_name(name: &str) -> bool {
    matches!(
        name,
        "MOVE" | "LEFTDOWN" | "LEFTUP" | "LEFTDRAG" | "RIGHTDOWN" | "RIGHTUP" | "RIGHTDRAG"
    )
}

fn cursor_before(left: &Cursor, right: &Cursor) -> bool {
    left.line_idx < right.line_idx
        || (left.line_idx == right.line_idx && left.cmd_idx < right.cmd_idx)
}

fn cursor_after(left: &Cursor, right: &Cursor) -> bool {
    left.line_idx > right.line_idx
        || (left.line_idx == right.line_idx && left.cmd_idx > right.cmd_idx)
}

fn cursor_in_exited_block(cursor: &Cursor, start: &Cursor, target: &Cursor) -> bool {
    (cursor == start || cursor_after(cursor, start)) && cursor_before(cursor, target)
}

fn restore_numeric_bindings(
    variables: &mut HashMap<String, f64>,
    saved: HashMap<String, Option<f64>>,
) {
    for (name, value) in saved {
        if let Some(value) = value {
            variables.insert(name, value);
        } else {
            variables.remove(&name);
        }
    }
}

fn restore_string_bindings(
    variables: &mut HashMap<String, String>,
    saved: HashMap<String, Option<String>>,
) {
    for (name, value) in saved {
        if let Some(value) = value {
            variables.insert(name, value);
        } else {
            variables.remove(&name);
        }
    }
}

fn restore_array_bindings(
    arrays: &mut HashMap<String, ArrayValue>,
    saved: HashMap<String, Option<ArrayValue>>,
) {
    for (name, value) in saved {
        if let Some(value) = value {
            arrays.insert(name, value);
        } else {
            arrays.remove(&name);
        }
    }
}

fn tokenize_mat_input_line(line: &str) -> (Vec<String>, bool) {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        if in_quotes {
            current.push(ch);
            if ch == '"' {
                in_quotes = false;
            }
            continue;
        }
        if ch == '"' {
            in_quotes = true;
            current.push(ch);
        } else if ch == ',' {
            tokens.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    tokens.push(current.trim().to_string());
    (tokens, in_quotes)
}

#[cfg(windows)]
fn console_read_key_code() -> Option<u8> {
    #[link(name = "msvcrt")]
    extern "C" {
        fn _kbhit() -> i32;
        fn _getch() -> i32;
    }

    unsafe {
        if _kbhit() == 0 {
            return None;
        }
        let first = _getch();
        if first == 0 || first == 224 {
            let second = _getch();
            return (0..=255).contains(&second).then_some(second as u8);
        }
        (0..=255).contains(&first).then_some(first as u8)
    }
}

#[cfg(not(windows))]
fn console_read_key_code() -> Option<u8> {
    console::read_runtime_key_code()
}

fn read_text_file(path: &Path) -> BasicResult<String> {
    fs::read_to_string(path).map_err(file_io_error)
}

fn file_io_error(err: io::Error) -> BasicError {
    if err.kind() == io::ErrorKind::NotFound {
        BasicError::new(ErrorCode::FileNotFound)
    } else {
        BasicError::new(ErrorCode::InvalidValue).with_detail(err.to_string())
    }
}

fn extract_quoted_path_text(value: &str) -> BasicResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        let path = &trimmed[1..trimmed.len() - 1];
        if path.is_empty() {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        Ok(path.to_string())
    } else {
        Err(BasicError::new(ErrorCode::MissingQuotes))
    }
}

fn extract_quoted_text_allow_empty(value: &str) -> BasicResult<String> {
    let trimmed = value.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        Ok(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        Err(BasicError::new(ErrorCode::MissingQuotes))
    }
}

fn resolve_wildcard_pattern(value: &str) -> BasicResult<String> {
    let wildcard = extract_quoted_text_allow_empty(value)?;
    if wildcard.contains('/') || wildcard.contains('\\') || wildcard.contains("..") {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    if !wildcard
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '?' | '*'))
    {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    Ok(if wildcard.is_empty() {
        "*.bas".to_string()
    } else {
        wildcard
    })
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn basic_error_number(err: &BasicError) -> i32 {
    if let Some(detail) = &err.detail {
        if let Some(rest) = detail.strip_prefix("Error ") {
            if let Ok(number) = rest.trim().parse::<i32>() {
                return number;
            }
        }
    }
    err.code.number()
}

fn error_message_without_dot(err: &BasicError) -> String {
    err.detail
        .as_deref()
        .unwrap_or_else(|| err.code.message())
        .to_string()
}

fn color_number_from_value(value: Value) -> BasicResult<i32> {
    match value {
        Value::Number(n) => {
            let color = n as i32;
            if color < 0 || color > 0x00ff_ffff {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            Ok(color)
        }
        Value::Str(text) => color_number_from_string(&text),
        Value::ArrayRef(_) => Err(BasicError::new(ErrorCode::TypeMismatch)),
    }
}

fn color_number_from_string(text: &str) -> BasicResult<i32> {
    let trimmed = text.trim();
    if let Some((r, g, b)) = parse_rgb_string_components(trimmed)? {
        return rgb_number(r, g, b);
    }
    if let Some(hex) = trimmed.strip_prefix('#') {
        let expanded = if hex.len() == 3 {
            let mut out = String::with_capacity(6);
            for ch in hex.chars() {
                out.push(ch);
                out.push(ch);
            }
            out
        } else {
            hex.to_string()
        };
        if expanded.len() == 6 && expanded.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return i32::from_str_radix(&expanded, 16)
                .map_err(|_| BasicError::new(ErrorCode::InvalidArgument));
        }
    }
    if let Some((r, g, b)) = named_color_rgb(trimmed) {
        return rgb_number(r, g, b);
    }
    Err(BasicError::new(ErrorCode::InvalidArgument))
}

fn parse_rgb_string(text: &str) -> BasicResult<(i32, i32, i32)> {
    if let Some(rgb) = parse_rgb_string_components(text)? {
        return Ok(rgb);
    }
    if let Some((r, g, b)) = named_color_rgb(text) {
        return Ok((r, g, b));
    }
    Err(BasicError::new(ErrorCode::InvalidArgument))
}

fn parse_rgb_string_components(text: &str) -> BasicResult<Option<(i32, i32, i32)>> {
    if !text.contains(',') {
        return Ok(None);
    }
    let parts: Vec<_> = text.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    let mut rgb = [0i32; 3];
    for (idx, part) in parts.iter().enumerate() {
        let component = part
            .parse::<f64>()
            .map_err(|_| BasicError::new(ErrorCode::InvalidArgument))?
            .round() as i32;
        if !(0..=255).contains(&component) {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        rgb[idx] = component;
    }
    Ok(Some((rgb[0], rgb[1], rgb[2])))
}

fn clip_graph_segment(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
) -> Option<(f64, f64, f64, f64)> {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let p = [-dx, dx, -dy, dy];
    let q = [x0 - xmin, xmax - x0, y0 - ymin, ymax - y0];
    let mut u1 = 0.0;
    let mut u2 = 1.0;
    for (pi, qi) in p.into_iter().zip(q) {
        if pi == 0.0 {
            if qi < 0.0 {
                return None;
            }
            continue;
        }
        let u = qi / pi;
        if pi < 0.0 {
            if u > u2 {
                return None;
            }
            if u > u1 {
                u1 = u;
            }
        } else {
            if u < u1 {
                return None;
            }
            if u < u2 {
                u2 = u;
            }
        }
    }
    Some((x0 + u1 * dx, y0 + u1 * dy, x0 + u2 * dx, y0 + u2 * dy))
}

fn graph_is_discontinuity_bridge(
    y0: f64,
    y1: f64,
    y_mid: Option<f64>,
    y_min: f64,
    y_max: f64,
) -> bool {
    let opposite_outside = (y0 > y_max && y1 < y_min) || (y0 < y_min && y1 > y_max);
    if !opposite_outside {
        return false;
    }
    match y_mid {
        Some(value) => value < y_min || value > y_max,
        None => true,
    }
}

fn named_color_rgb(name: &str) -> Option<(i32, i32, i32)> {
    let rgb = match name.trim().to_ascii_lowercase().as_str() {
        "black" => (0x00, 0x00, 0x00),
        "white" => (0xff, 0xff, 0xff),
        "red" => (0xff, 0x00, 0x00),
        "green" => (0x00, 0x80, 0x00),
        "blue" => (0x00, 0x00, 0xff),
        "yellow" => (0xff, 0xff, 0x00),
        "magenta" => (0xff, 0x00, 0xff),
        "cyan" => (0x00, 0xff, 0xff),
        "darkorange" => (0xff, 0x8c, 0x00),
        "purple" => (0x80, 0x00, 0x80),
        "brown" => (0xa5, 0x2a, 0x2a),
        "gray" => (0x80, 0x80, 0x80),
        "lightgreen" => (0x90, 0xee, 0x90),
        "lightblue" => (0xad, 0xd8, 0xe6),
        "lightgray" => (0xd3, 0xd3, 0xd3),
        "mediumpurple" => (0x93, 0x70, 0xdb),
        "lightcyan" => (0xe0, 0xff, 0xff),
        "hotpink" => (0xff, 0x69, 0xb4),
        "gold" => (0xff, 0xd7, 0x00),
        "indigo" => (0x4b, 0x00, 0x82),
        "violet" => (0xee, 0x82, 0xee),
        "steelblue" => (0x46, 0x82, 0xb4),
        "salmon" => (0xfa, 0x80, 0x72),
        "khaki" => (0xf0, 0xe6, 0x8c),
        "pink" => (0xff, 0xc0, 0xcb),
        "olive" => (0x80, 0x80, 0x00),
        "lime" => (0x00, 0xff, 0x00),
        "navy" => (0x00, 0x00, 0x80),
        "teal" => (0x00, 0x80, 0x80),
        "tan" => (0xd2, 0xb4, 0x8c),
        "maroon" => (0x80, 0x00, 0x00),
        "ivory" => (0xff, 0xff, 0xf0),
        _ => return None,
    };
    Some((rgb.0, rgb.1, rgb.2))
}

fn strip_wrapping_parens(source: &str) -> &str {
    let trimmed = source.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return trimmed;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    for (idx, ch) in trimmed.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && idx != trimmed.len() - 1 {
                    return trimmed;
                }
            }
            _ => {}
        }
    }
    &trimmed[1..trimmed.len() - 1]
}

fn whole_function_argument<'a>(source: &'a str, function: &str) -> Option<&'a str> {
    let trimmed = source.trim();
    let prefix_len = function.len();
    let Some(prefix) = trimmed.get(..prefix_len) else {
        return None;
    };
    if trimmed.len() <= prefix_len + 1
        || !prefix.eq_ignore_ascii_case(function)
        || !trimmed[prefix_len..].starts_with('(')
        || !trimmed.ends_with(')')
    {
        return None;
    }
    if strip_wrapping_parens(&trimmed[prefix_len..]) == &trimmed[prefix_len..] {
        return None;
    }
    Some(&trimmed[prefix_len + 1..trimmed.len() - 1])
}

fn looks_like_non_fn_function_call(source: &str) -> bool {
    let trimmed = source.trim();
    let Some(open) = trimmed.find('(') else {
        return false;
    };
    if !trimmed.ends_with(')') {
        return false;
    }
    let name = trimmed[..open].trim();
    is_basic_identifier(name)
        && !name.eq_ignore_ascii_case("TRN")
        && !name.eq_ignore_ascii_case("INV")
        && !name.to_ascii_uppercase().starts_with("FN")
}

fn mat_expr_mentions_array(source: &str, arrays: &HashMap<String, ArrayValue>) -> bool {
    let mut token = String::new();
    let mut in_string = false;
    for ch in source.chars().chain(std::iter::once(' ')) {
        if ch == '"' {
            in_string = !in_string;
            token.clear();
            continue;
        }
        if in_string {
            continue;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            token.push(ch.to_ascii_uppercase());
        } else {
            if arrays.contains_key(&token) {
                return true;
            }
            token.clear();
        }
    }
    false
}

fn mat_expr_has_array_before_function(source: &str, arrays: &HashMap<String, ArrayValue>) -> bool {
    let mut seen_array = false;
    let mut token = String::new();
    let mut in_string = false;
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0usize;
    while i <= chars.len() {
        let ch = chars.get(i).copied().unwrap_or(' ');
        if ch == '"' {
            in_string = !in_string;
            token.clear();
            i += 1;
            continue;
        }
        if !in_string && (ch.is_ascii_alphanumeric() || ch == '_' || ch == '$') {
            token.push(ch.to_ascii_uppercase());
        } else if !token.is_empty() {
            let name = token.clone();
            if arrays.contains_key(&name) {
                seen_array = true;
            }
            if seen_array
                && chars.get(i..).is_some_and(|rest| {
                    rest.iter().skip_while(|c| c.is_whitespace()).next() == Some(&'(')
                })
                && !name.starts_with("FN")
                && name != "TRN"
                && name != "INV"
                && !arrays.contains_key(&name)
            {
                return true;
            }
            token.clear();
        }
        i += 1;
    }
    false
}

fn scalar_times_matrix_div_scalar(source: &str, arrays: &HashMap<String, ArrayValue>) -> bool {
    let Some((mul_pos, '*')) = find_top_level_mat_operator(source, &['*']) else {
        return false;
    };
    let Some((_, '/')) = find_top_level_mat_operator(&source[mul_pos + 1..], &['/']) else {
        return false;
    };
    !mat_expr_mentions_array(&source[..mul_pos], arrays)
        && mat_expr_mentions_array(&source[mul_pos + 1..], arrays)
}

fn find_top_level_mat_operator(source: &str, ops: &[char]) -> Option<(usize, char)> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut candidate = None;
    for (idx, ch) in source.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth == 0 && ops.contains(&ch) => {
                if (ch == '+' || ch == '-') && is_unary_operator_at(source, idx) {
                    continue;
                }
                if ch == '-' && source[..idx].ends_with(['E', 'e']) {
                    continue;
                }
                candidate = Some((idx, ch));
            }
            _ => {}
        }
    }
    candidate
}

fn is_unary_operator_at(source: &str, idx: usize) -> bool {
    let prev = source[..idx].chars().rev().find(|ch| !ch.is_whitespace());
    matches!(
        prev,
        None | Some('(' | '+' | '-' | '*' | '/' | '^' | ',' | '=')
    )
}

fn parse_mat_print_item(item: &str) -> BasicResult<(MatOrientation, String)> {
    let trimmed = item.trim();
    let upper = trimmed.to_ascii_uppercase();
    if upper.starts_with("ROW ") {
        let name = trimmed[4..].trim().to_ascii_uppercase();
        if !is_basic_identifier(&name) {
            return Err(BasicError::new(ErrorCode::ForbiddenExpression));
        }
        return Ok((MatOrientation::Row, name));
    }
    if upper.starts_with("COL ") {
        let name = trimmed[4..].trim().to_ascii_uppercase();
        if !is_basic_identifier(&name) {
            return Err(BasicError::new(ErrorCode::ForbiddenExpression));
        }
        return Ok((MatOrientation::Col, name));
    }
    let name = upper;
    if !is_basic_identifier(&name) {
        return Err(BasicError::new(ErrorCode::ForbiddenExpression));
    }
    Ok((MatOrientation::Normal, name))
}

fn classify_if_branch_command(command: &str) -> Option<IfBranchKind> {
    let upper = command.trim().to_ascii_uppercase();
    if upper.starts_with("ELSEIF ") {
        Some(IfBranchKind::ElseIf)
    } else if upper == "ELSE" {
        Some(IfBranchKind::Else)
    } else if upper == "END IF" || upper == "ENDIF" {
        Some(IfBranchKind::EndIf)
    } else {
        None
    }
}

fn is_multiline_if_start(command: &str) -> bool {
    let trimmed = command.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("IF ") {
        return false;
    }
    let Some((_pos, then_end)) = find_then_keyword(&upper) else {
        return false;
    };
    trimmed[then_end..].trim().is_empty()
}

fn find_then_keyword(upper: &str) -> Option<(usize, usize)> {
    let pos = upper.find(" THEN")?;
    let end = pos + 5;
    if upper[end..]
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace())
    {
        return None;
    }
    Some((pos, end))
}

fn find_keyword_after_if(upper: &str, keyword: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    for (idx, ch) in upper.char_indices().skip(2) {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth == 0 && upper[idx..].starts_with(keyword) => return Some(idx),
            _ => {}
        }
    }
    None
}

fn find_assignment_equal(source: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    for (i, ch) in chars.iter().copied() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            '=' if depth == 0 => {
                let prev = source[..i].chars().rev().find(|c| !c.is_whitespace());
                let next = source[i + 1..].chars().find(|c| !c.is_whitespace());
                if !matches!(prev, Some('<' | '>')) && !matches!(next, Some('=' | '<' | '>')) {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn contains_double_equal_top_level(source: &str) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut previous_equal = false;
    for ch in source.chars() {
        if ch == '"' {
            in_string = !in_string;
            previous_equal = false;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => {
                depth += 1;
                previous_equal = false;
            }
            ')' => {
                depth -= 1;
                previous_equal = false;
            }
            '=' if depth == 0 => {
                if previous_equal {
                    return true;
                }
                previous_equal = true;
            }
            ch if ch.is_whitespace() => {}
            _ => previous_equal = false,
        }
    }
    false
}

fn split_assignment_targets_and_rhs(source: &str) -> Option<(Vec<&str>, &str)> {
    let mut targets = Vec::new();
    let mut start = 0usize;
    loop {
        let eq = find_assignment_equal_from(source, start)?;
        let candidate = source[start..eq].trim();
        if !is_assignment_target(candidate) {
            if targets.is_empty() {
                return None;
            }
            return Some((targets, source[start..].trim()));
        }
        targets.push(candidate);
        start = eq + 1;
        if find_assignment_equal_from(source, start).is_none() {
            return Some((targets, source[start..].trim()));
        }
    }
}

fn compile_for_statement(source: &str) -> BasicResult<CompiledFor> {
    let upper = source.to_ascii_uppercase();
    let Some(eq) = source.find('=') else {
        return Err(BasicError::new(ErrorCode::Syntax));
    };
    let var = source[..eq].trim().to_ascii_uppercase();
    if !is_basic_identifier(&var) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    let Some(to_pos) = upper[eq + 1..].find(" TO ") else {
        return Err(BasicError::new(ErrorCode::Syntax));
    };
    let to_pos = eq + 1 + to_pos;
    let start_expr = source[eq + 1..to_pos].trim();
    let after_to = source[to_pos + 4..].trim();
    let after_to_upper = after_to.to_ascii_uppercase();
    let (end_expr, step_expr) = if let Some(step_pos) = after_to_upper.find(" STEP ") {
        (&after_to[..step_pos], Some(after_to[step_pos + 6..].trim()))
    } else {
        (after_to, None)
    };
    Ok(CompiledFor {
        var,
        start: compile_expression(start_expr)?,
        end: compile_expression(end_expr.trim())?,
        step: step_expr.map(compile_expression).transpose()?,
    })
}

fn compile_mid_assignment_statement(source: &str) -> BasicResult<CompiledMidAssignment> {
    let pos = find_assignment_equal(source).ok_or_else(|| BasicError::new(ErrorCode::Syntax))?;
    let lhs = source[..pos].trim();
    let rhs = source[pos + 1..].trim();
    let lhs_upper = lhs.to_ascii_uppercase();
    if !lhs_upper.starts_with("MID$(") || !lhs.ends_with(')') {
        return Err(BasicError::new(ErrorCode::Syntax));
    }
    let args = split_arguments(&lhs[5..lhs.len() - 1]);
    if args.len() < 2 || args.len() > 3 {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    let target = simple_string_variable_name(args[0].trim())
        .ok_or_else(|| BasicError::new(ErrorCode::Syntax))?;
    Ok(CompiledMidAssignment {
        target,
        start: compile_expression(args[1].trim())?,
        count: args
            .get(2)
            .map(|arg| compile_expression(arg.trim()))
            .transpose()?,
        rhs: compile_expression(rhs)?,
    })
}

fn compile_draw_relative2(source: &str) -> BasicResult<(Expr, Expr)> {
    let args = split_arguments(source);
    if args.len() != 2 || args.iter().any(|arg| arg.trim().is_empty()) {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    Ok((
        compile_expression(args[0].trim())?,
        compile_expression(args[1].trim())?,
    ))
}

fn compiled_string_char_assignment(
    compiled: &CompiledAssignment,
) -> Option<(String, String, Expr)> {
    if compiled.targets.len() != 1 || !compiled.rhs_is_string {
        return None;
    }
    let CompiledLValue::Scalar {
        name: target,
        is_string: true,
    } = &compiled.targets[0]
    else {
        return None;
    };
    let Expr::ArrayOrCall { name, args } = &compiled.rhs else {
        return None;
    };
    if !name.eq_ignore_ascii_case("MID$") || args.len() != 3 {
        return None;
    }
    let source = expr_simple_string_variable_name(&args[0])?;
    let Expr::Number(count) = &args[2] else {
        return None;
    };
    if *count != 1.0 {
        return None;
    }
    Some((target.clone(), source, args[1].clone()))
}

fn expr_simple_string_variable_name(expr: &Expr) -> Option<String> {
    let Expr::Var(name) = expr else {
        return None;
    };
    simple_string_variable_name(name)
}

fn compile_assignment_statement(source: &str) -> BasicResult<CompiledAssignment> {
    let (targets, rhs) = if let Some((targets, rhs)) = split_assignment_targets_and_rhs(source) {
        (targets, rhs)
    } else {
        let pos =
            find_assignment_equal(source).ok_or_else(|| BasicError::new(ErrorCode::Syntax))?;
        (vec![source[..pos].trim()], source[pos + 1..].trim())
    };
    if targets
        .iter()
        .any(|target| assignment_target_is_reserved_function(target))
    {
        return Err(BasicError::new(ErrorCode::Syntax));
    }
    let rhs_is_string = targets
        .iter()
        .any(|target| assignment_target_is_string(target));
    let compiled_targets = targets
        .into_iter()
        .map(compile_assignment_lvalue)
        .collect::<BasicResult<Vec<_>>>()?;
    Ok(CompiledAssignment {
        targets: compiled_targets,
        rhs: compile_expression(rhs)?,
        rhs_is_string,
    })
}

fn compile_assignment_lvalue(source: &str) -> BasicResult<CompiledLValue> {
    let lhs = source.trim().to_ascii_uppercase();
    if lhs.starts_with("MID$(") {
        return Err(BasicError::new(ErrorCode::Syntax));
    }
    if let Some(open) = lhs.find('(') {
        let close = lhs
            .rfind(')')
            .ok_or_else(|| BasicError::new(ErrorCode::Syntax))?;
        if close != lhs.len() - 1 {
            return Err(BasicError::new(ErrorCode::Syntax));
        }
        let name = lhs[..open].trim().to_string();
        if !is_basic_identifier(&name) {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        let indexes = split_arguments(&lhs[open + 1..close])
            .into_iter()
            .map(|arg| compile_expression(&arg))
            .collect::<BasicResult<Vec<_>>>()?;
        return Ok(CompiledLValue::Array {
            is_string: name.ends_with('$'),
            name,
            indexes,
        });
    }
    if !is_basic_identifier(&lhs) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    Ok(CompiledLValue::Scalar {
        is_string: lhs.ends_with('$'),
        name: lhs,
    })
}

fn find_assignment_equal_from(source: &str, start_offset: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    for (i, ch) in source[start_offset..].char_indices() {
        let i = start_offset + i;
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            '=' if depth == 0 => {
                let prev = source[..i].chars().rev().find(|c| !c.is_whitespace());
                let next = source[i + 1..].chars().find(|c| !c.is_whitespace());
                if !matches!(prev, Some('<' | '>')) && !matches!(next, Some('=' | '<' | '>')) {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_assignment_target(source: &str) -> bool {
    let trimmed = source.trim();
    if let Some(open) = trimmed.find('(') {
        if !trimmed.ends_with(')') {
            return false;
        }
        return is_basic_identifier(trimmed[..open].trim());
    }
    is_basic_identifier(trimmed)
}

fn split_first_top_level(source: &str, separator: char) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut in_string = false;
    for (i, ch) in source.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if ch == separator && depth == 0 => {
                return Some((&source[..i], &source[i + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

fn split_print_items(source: &str) -> Vec<(&str, Option<char>)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    for (i, ch) in source.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' | ',' if depth == 0 => {
                out.push((&source[start..i], Some(ch)));
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start < source.len() {
        out.push((&source[start..], None));
    }
    out
}

fn renumber_line_references(code: &str, mapping: &HashMap<i32, i32>) -> String {
    let mut out = String::new();
    let mut i = 0usize;
    let mut in_string = false;
    while i < code.len() {
        let ch = code[i..].chars().next().unwrap();
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if !in_string && ch == '\'' {
            out.push_str(&code[i..]);
            break;
        }
        if !in_string && is_reference_ident_start(ch) {
            let start = i;
            i += ch.len_utf8();
            while i < code.len() {
                let next = code[i..].chars().next().unwrap();
                if !is_reference_ident_char(next) {
                    break;
                }
                i += next.len_utf8();
            }
            let ident = &code[start..i];
            out.push_str(ident);
            if matches!(
                ident.to_ascii_uppercase().as_str(),
                "GOTO" | "GOSUB" | "THEN" | "ELSE" | "RESTORE" | "RESUME"
            ) {
                i = copy_renumbered_line_list(code, i, &mut out, mapping);
            }
            continue;
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn copy_renumbered_line_list(
    code: &str,
    mut i: usize,
    out: &mut String,
    mapping: &HashMap<i32, i32>,
) -> usize {
    loop {
        while i < code.len() {
            let ch = code[i..].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            out.push(ch);
            i += ch.len_utf8();
        }
        let number_start = i;
        while i < code.len() {
            let ch = code[i..].chars().next().unwrap();
            if !ch.is_ascii_digit() {
                break;
            }
            i += ch.len_utf8();
        }
        if number_start == i {
            return i;
        }
        let raw = &code[number_start..i];
        if let Ok(old) = raw.parse::<i32>() {
            if let Some(new_no) = mapping.get(&old) {
                out.push_str(&new_no.to_string());
            } else {
                out.push_str(raw);
            }
        } else {
            out.push_str(raw);
        }
        let mut probe = i;
        while probe < code.len() {
            let ch = code[probe..].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            probe += ch.len_utf8();
        }
        if probe < code.len() && code[probe..].starts_with(',') {
            out.push_str(&code[i..probe + 1]);
            i = probe + ','.len_utf8();
            continue;
        }
        return i;
    }
}

fn is_reference_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_reference_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    fn inner(pattern: &[u8], text: &[u8]) -> bool {
        if pattern.is_empty() {
            return text.is_empty();
        }
        match pattern[0] {
            b'*' => inner(&pattern[1..], text) || (!text.is_empty() && inner(pattern, &text[1..])),
            b'?' => !text.is_empty() && inner(&pattern[1..], &text[1..]),
            ch => {
                !text.is_empty()
                    && ch.eq_ignore_ascii_case(&text[0])
                    && inner(&pattern[1..], &text[1..])
            }
        }
    }
    inner(pattern.as_bytes(), text.as_bytes())
}

fn format_using_simple(value: f64, fmt: &str) -> String {
    let european_separators = fmt.starts_with(',');
    let fmt = if european_separators {
        &fmt[','.len_utf8()..]
    } else {
        fmt
    };
    let decimal_separator = if european_separators { ',' } else { '.' };
    let thousands_separator = if european_separators { '.' } else { ',' };

    if fmt.eq_ignore_ascii_case("0.00^^^^") {
        let raw = format!("{value:.2E}");
        if let Some((mantissa, exponent)) = raw.split_once('E') {
            let exp = exponent.parse::<i32>().unwrap_or(0);
            let sign = if exp < 0 { '-' } else { '+' };
            let mantissa = if european_separators {
                mantissa.replace('.', ",")
            } else {
                mantissa.to_string()
            };
            return format!("{mantissa}E{sign}{:02}", exp.abs());
        }
        return raw;
    }
    if fmt == "#,###,###" {
        let grouped = group_thousands(value.round() as i64, thousands_separator);
        return format!("{grouped:>9}");
    }
    let Some(first_digit) = fmt.find(|c| matches!(c, '#' | '0')) else {
        return format_basic_number(value);
    };
    let last_digit = fmt
        .char_indices()
        .filter(|(_, c)| matches!(*c, '#' | '0'))
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(first_digit + 1);
    let mut prefix = &fmt[..first_digit];
    let body = &fmt[first_digit..last_digit];
    let suffix = &fmt[last_digit..];
    let force_sign = prefix.ends_with('+');
    if force_sign {
        prefix = &prefix[..prefix.len() - 1];
    }
    let decimal = body.find('.');
    let int_mask = decimal.map_or(body, |pos| &body[..pos]);
    let frac_mask = decimal.map_or("", |pos| &body[pos + 1..]);
    let frac_digits = frac_mask
        .chars()
        .filter(|c| matches!(*c, '#' | '0'))
        .count();
    let abs = value.abs();
    let raw = if frac_digits == 0 {
        format!("{abs:.0}")
    } else {
        format!("{abs:.frac_digits$}")
    };
    let rounded_nonzero = raw.chars().any(|ch| ch.is_ascii_digit() && ch != '0');
    let negative = value < 0.0 && rounded_nonzero;
    let (int_part, frac_part) = raw.split_once('.').unwrap_or((raw.as_str(), ""));
    let signed_int = format_using_integer(
        int_part,
        int_mask,
        negative,
        force_sign,
        thousands_separator,
    );
    let numeric = if frac_digits == 0 {
        signed_int
    } else {
        format!("{signed_int}{decimal_separator}{frac_part}")
    };
    format!("{prefix}{numeric}{suffix}")
}

fn valid_using_format(fmt: &str) -> bool {
    !fmt.is_empty() && fmt.chars().any(|ch| matches!(ch, '#' | '0'))
}

fn format_using_integer(
    digits: &str,
    mask: &str,
    negative: bool,
    force_sign: bool,
    thousands_separator: char,
) -> String {
    let positions: Vec<usize> = mask
        .char_indices()
        .filter(|(_, ch)| matches!(*ch, '#' | '0'))
        .map(|(idx, _)| idx)
        .collect();
    let slots = positions.len();
    if slots == 0 {
        return digits.to_string();
    }
    if mask.contains(',') {
        let grouped = group_thousands(digits.parse::<i64>().unwrap_or(0), thousands_separator);
        let mut text = if negative {
            format!("-{grouped}")
        } else if force_sign {
            format!("+{grouped}")
        } else {
            grouped
        };
        let width = mask.chars().count();
        if text.len() < width {
            text = format!("{}{}", " ".repeat(width - text.len()), text);
        }
        return text;
    }
    let zero_pad = mask.chars().any(|ch| ch == '0');
    let sign = if negative {
        Some('-')
    } else if force_sign {
        Some('+')
    } else {
        None
    };
    if digits.len() > slots || (negative && !force_sign && zero_pad && slots == 1) {
        let mut out = apply_integer_literals_from_right(digits, mask);
        if let Some(sign) = sign {
            out.insert(0, sign);
        }
        return out;
    }

    if negative && !force_sign && zero_pad {
        let mut out = mask.to_string();
        let first = positions[0];
        out.replace_range(first..first + 1, "-");
        let remaining_slots = slots.saturating_sub(1);
        let padded = digits
            .to_string()
            .pad_left(remaining_slots, '0')
            .chars()
            .collect::<Vec<_>>();
        for (slot_idx, pos) in positions.iter().skip(1).enumerate() {
            let ch = padded.get(slot_idx).copied().unwrap_or('0');
            out.replace_range(*pos..*pos + 1, &ch.to_string());
        }
        return out;
    }

    let pad = if zero_pad { '0' } else { ' ' };
    let padded = digits
        .to_string()
        .pad_left(slots, pad)
        .chars()
        .collect::<Vec<_>>();
    let mut out = mask.to_string();
    for (slot_idx, pos) in positions.iter().enumerate() {
        let ch = padded.get(slot_idx).copied().unwrap_or(pad);
        out.replace_range(*pos..*pos + 1, &ch.to_string());
    }
    if let Some(sign) = sign {
        if force_sign {
            out.insert(0, sign);
        } else if zero_pad {
            out.insert(0, sign);
        } else if let Some(first_digit) = out.find(|ch: char| ch.is_ascii_digit()) {
            if first_digit > 0 {
                out.replace_range(first_digit - 1..first_digit, &sign.to_string());
            } else {
                out.insert(0, sign);
            }
        } else {
            out.insert(0, sign);
        }
    }
    out
}

fn apply_integer_literals_from_right(digits: &str, mask: &str) -> String {
    let mut out = String::new();
    let mut digit_iter = digits.chars().rev();
    for ch in mask.chars().rev() {
        if matches!(ch, '#' | '0') {
            if let Some(digit) = digit_iter.next() {
                out.push(digit);
            }
        } else if !out.is_empty() {
            out.push(ch);
        }
    }
    for digit in digit_iter {
        out.push(digit);
    }
    out.chars().rev().collect()
}

trait PadLeft {
    fn pad_left(self, width: usize, ch: char) -> String;
}

impl PadLeft for String {
    fn pad_left(self, width: usize, ch: char) -> String {
        if self.len() >= width {
            self
        } else {
            format!("{}{}", ch.to_string().repeat(width - self.len()), self)
        }
    }
}

fn group_thousands(value: i64, separator: char) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (idx, ch) in digits.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(separator);
        }
        out.push(ch);
    }
    let mut grouped: String = out.chars().rev().collect();
    if negative {
        grouped.insert(0, '-');
    }
    grouped
}

fn split_else(rest: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    let mut if_depth = 0i32;
    let mut in_string = false;
    for (idx, ch) in rest.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            'I' | 'i' if depth == 0 && keyword_at(rest, idx, "IF") => {
                if_depth += 1;
            }
            'E' | 'e' if depth == 0 && keyword_at(rest, idx, "ELSE") => {
                if if_depth > 0 {
                    if_depth -= 1;
                } else {
                    let after_idx = idx + 4;
                    return (&rest[..idx], Some(rest[after_idx..].trim_start()));
                }
            }
            _ => {}
        }
    }
    (rest, None)
}

fn keyword_at(source: &str, idx: usize, keyword: &str) -> bool {
    let Some(tail) = source.get(idx..) else {
        return false;
    };
    if !tail
        .get(..keyword.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(keyword))
    {
        return false;
    }
    let before_ok = idx == 0
        || source[..idx]
            .chars()
            .last()
            .is_some_and(|c| c.is_whitespace() || c == ':');
    let after_idx = idx + keyword.len();
    let after_ok = after_idx >= source.len()
        || source[after_idx..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == ':');
    before_ok && after_ok
}

fn parse_data_items(source: &str) -> Vec<Value> {
    split_top_level(&strip_comment(source), &[','])
        .into_iter()
        .map(|item| {
            let item = item.trim();
            if item.starts_with('"') && item.ends_with('"') && item.len() >= 2 {
                Value::string(item[1..item.len() - 1].to_string())
            } else if item.is_empty() {
                Value::string("")
            } else if let Some(n) = parse_data_number(item) {
                Value::number(n)
            } else if let Ok(n) = item.parse::<f64>() {
                Value::number(n)
            } else {
                Value::string(item.to_string())
            }
        })
        .collect()
}

fn parse_data_number(item: &str) -> Option<f64> {
    let lower = item.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("&h") {
        if !hex.is_empty() && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return i64::from_str_radix(hex, 16).ok().map(|v| v as f64);
        }
    }
    if let Some(bin) = lower.strip_prefix("&x") {
        if !bin.is_empty() && bin.chars().all(|ch| matches!(ch, '0' | '1')) {
            return i64::from_str_radix(bin, 2).ok().map(|v| v as f64);
        }
    }
    None
}
