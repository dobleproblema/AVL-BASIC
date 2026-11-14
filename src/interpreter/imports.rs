use super::*;

#[derive(Debug)]
pub(super) struct ImportDeclaration {
    pub(super) line: i32,
    pub(super) path: String,
    pub(super) alias: String,
}

pub(super) fn import_argument(command: &str) -> Option<&str> {
    let command = command.trim();
    if !command.get(..6)?.eq_ignore_ascii_case("IMPORT") {
        return None;
    }
    let rest = &command[6..];
    if rest.starts_with('"') || rest.is_empty() {
        return Some(rest);
    }
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    (!rest.starts_with(['=', '('])).then_some(rest)
}

impl ImportDeclaration {
    pub(super) fn parse(line: i32, argument: &str) -> BasicResult<Self> {
        let invalid = || {
            BasicError::new(ErrorCode::Syntax)
                .with_detail("Expected IMPORT \"file.bas\" AS Alias.")
                .at_line(line)
        };
        let (path, tail) = argument
            .strip_prefix('"')
            .and_then(|text| text.split_once('"'))
            .ok_or_else(invalid)?;
        let mut words = tail.split_whitespace();
        let as_word = words.next().ok_or_else(invalid)?;
        let alias = words.next().ok_or_else(invalid)?;
        if path.is_empty()
            || !tail.starts_with(char::is_whitespace)
            || !as_word.eq_ignore_ascii_case("AS")
            || !is_import_alias(alias)
            || words.next().is_some()
        {
            return Err(invalid());
        }
        Ok(Self {
            line,
            path: path.to_string(),
            alias: alias.to_ascii_uppercase(),
        })
    }
}

pub(super) fn is_import_alias(name: &str) -> bool {
    is_basic_identifier(name) && !name.contains('$')
}

pub(super) fn contains_import(command: &str) -> bool {
    if import_argument(command).is_some() {
        return true;
    }
    if !first_word_is(command, "IF") {
        return false;
    }
    let command = command.trim();
    let upper = command.to_ascii_uppercase();
    let Some((_, rest)) = split_if_condition_and_rest(command, &upper) else {
        return false;
    };
    let (then_part, else_part) = split_else(rest);
    std::iter::once(then_part)
        .chain(else_part)
        .flat_map(split_commands)
        .any(|nested| contains_import(&nested))
}

// Only state keyed by a source line, variable name, or execution cursor
// changes at the module boundary. The console, graphics, and audio stay live.
macro_rules! module_state {
    ($($field:ident: $ty:ty),+ $(,)?) => {
        #[derive(Debug)]
        struct ModuleState {
            $($field: $ty),+
        }

        impl ModuleState {
            fn from_interpreter(interpreter: Interpreter) -> Self {
                let Interpreter { $($field),+, .. } = interpreter;
                Self { $($field),+ }
            }

            fn swap(&mut self, interpreter: &mut Interpreter) {
                $(std::mem::swap(&mut self.$field, &mut interpreter.$field);)+
            }
        }
    };
}

module_state! {
    program: Program,
    program_dir: Option<PathBuf>,
    command_cache: HashMap<i32, Vec<Rc<str>>>,
    compiled_command_cache: HashMap<i32, Vec<Rc<CachedCommand>>>,
    execution_source_cache: HashMap<i32, Vec<ExecutionMetadata>>,
    compiled_line_cache: Rc<[Rc<[Rc<CachedCommand>]>]>,
    line_index_cache: HashMap<i32, usize>,
    line_numbers_cache: Rc<[i32]>,
    line_labels: HashMap<(String, String), i32>,
    label_generation: u64,
    next_after_for_cache: HashMap<Cursor, Cursor>,
    wend_after_while_cache: HashMap<Cursor, Cursor>,
    next_if_branch_cache: HashMap<Cursor, IfBranch>,
    after_end_if_cache: HashMap<Cursor, Cursor>,
    numeric_variables: NumericVariables,
    string_variables: FastHashMap<String, String>,
    identifier_case: HashMap<String, String>,
    arrays: ArrayVariables,
    sub_runtime: Box<SubRuntimeState>,
    array_aliases: FastHashMap<String, String>,
    data: Vec<Value>,
    data_line_starts: HashMap<i32, usize>,
    data_pointer: usize,
    functions: HashMap<String, UserFunction>,
    routine_definition_skips: HashMap<Cursor, Cursor>,
    single_line_function_cache: FastHashMap<Rc<str>, Rc<CompiledSingleLineFunctionBody>>,
    subs: HashMap<String, Rc<UserSub>>,
    function_call_stack: Vec<Rc<str>>,
    sub_call_stack: Vec<Rc<str>>,
    fn_line_owner: HashMap<i32, String>,
    sub_line_owner: HashMap<i32, String>,
    expression_cache: HashMap<String, Rc<Expr>>,
    assignment_cache: HashMap<String, Rc<CompiledAssignment>>,
    pending_sounds: Vec<(Cursor, SoundNote)>,
    for_stack: Vec<ForFrame>,
    while_stack: Vec<WhileFrame>,
    if_stack: Vec<Cursor>,
    gosub_stack: Vec<GosubFrame>,
    active_line_plan: Option<Rc<RetainedLinePlan>>,
    return_line_plan: Option<Rc<RetainedLinePlan>>,
    if_gosub_base: usize,
    pending_if_branch: Option<Cursor>,
    stopped_cursor: Option<Cursor>,
    pause_deadline: Option<(Cursor, Instant)>,
    current_line: Option<i32>,
}

#[derive(Debug)]
pub(super) struct ImportedModule {
    path: PathBuf,
    state: ModuleState,
    function_locations: HashMap<String, (i32, usize)>,
}

#[derive(Debug)]
pub(super) struct ActiveModule {
    pub(super) path: PathBuf,
    pub(super) function_base: usize,
    pub(super) sub_base: usize,
    pub(super) gosub_depth: usize,
}

impl Interpreter {
    pub(super) fn prepare_imports(&mut self, imports: Vec<ImportDeclaration>) -> BasicResult<()> {
        let mut modules = HashMap::new();
        for import in imports {
            let result = self.load_import(&import);
            let module = result.map_err(|error| {
                let message = error.display_for_basic();
                BasicError::new(error.code)
                    .at_line(import.line)
                    .with_detail(format!("IMPORT {:?}: {message}", import.path))
            })?;
            modules.insert(import.alias, module);
        }
        self.imported_modules = modules;
        Ok(())
    }

    fn load_import(&self, import: &ImportDeclaration) -> BasicResult<ImportedModule> {
        let base = self.program_dir.as_deref().unwrap_or(&self.current_dir);
        let path = self.resolve_bas_path_text(&import.path, base)?;
        let mut source = Interpreter::new();
        source.program = read_program_file(&path)?;
        source.program_dir = path.parent().map(Path::to_path_buf);
        source.active_module = Some(ActiveModule {
            path: path.clone(),
            function_base: 0,
            sub_base: 0,
            gosub_depth: 0,
        });
        source.refresh_identifier_case_from_program();
        source.rebuild_data();
        source.rebuild_command_cache();
        source.ensure_routine_catalog()?;

        let function_locations = source
            .routine_definition_skips
            .keys()
            .filter_map(|cursor| {
                let line = source.line_numbers_cache[cursor.line_idx];
                let command = source.command_cache.get(&line)?.get(cursor.cmd_idx)?;
                let header = command.get(4..)?.split('=').next()?.trim();
                let (name, _) = parse_function_header(header).ok()?;
                Some((name, (line, cursor.cmd_idx)))
            })
            .collect();
        Ok(ImportedModule {
            path,
            state: ModuleState::from_interpreter(source),
            function_locations,
        })
    }

    pub(super) fn call_imported_function(
        &mut self,
        alias: &str,
        function: &str,
        args: Vec<Value>,
    ) -> BasicResult<Value> {
        let module = self.import_for_call(alias)?;
        let (line, command) = module
            .function_locations
            .get(function)
            .copied()
            .ok_or_else(|| self.undefined_imported_routine(alias, function))?;
        let (value, array) = self.with_import(alias, args, |interpreter, args| {
            interpreter.current_line = Some(line);
            let single_line = matches!(
                interpreter.functions.get(function),
                Some(UserFunction::Single { .. })
            );
            let value = interpreter
                .call_user_function(function, args)
                .map_err(|error| {
                    if single_line {
                        interpreter.with_numbered_statement(error, line, command)
                    } else {
                        error
                    }
                })?;
            let array = match &value {
                Value::ArrayRef(name) => interpreter.arrays.get(name).cloned(),
                _ => None,
            };
            Ok((value, array))
        })?;
        if let Some(array) = array {
            let key = format!("{alias}.{function}");
            self.arrays.insert(key.clone(), array);
            Ok(Value::ArrayRef(key))
        } else {
            Ok(value)
        }
    }

    pub(super) fn call_imported_sub(
        &mut self,
        alias: &str,
        name: &str,
        args: Vec<Value>,
    ) -> BasicResult<()> {
        let module = self.import_for_call(alias)?;
        let line = module
            .state
            .subs
            .get(name)
            .map(|sub| module.state.line_numbers_cache[sub.start.line_idx - 1])
            .ok_or_else(|| self.undefined_imported_routine(alias, name))?;
        self.with_import(alias, args, |interpreter, args| {
            interpreter.current_line = Some(line);
            interpreter.call_multiline_sub(name, &mut args.into_iter())
        })
    }

    fn import_for_call(&self, alias: &str) -> BasicResult<&ImportedModule> {
        if self.active_module.is_some() {
            return Err(self
                .err(ErrorCode::Undefined)
                .with_detail("Import aliases are available in the main program."));
        }
        self.imported_modules.get(alias).ok_or_else(|| {
            self.err(ErrorCode::Undefined)
                .with_detail(format!("Undefined import alias '{alias}'."))
        })
    }

    fn undefined_imported_routine(&self, alias: &str, name: &str) -> BasicError {
        self.err(ErrorCode::Undefined)
            .with_detail(format!("Undefined imported routine '{alias}.{name}'."))
    }

    fn with_import<T>(
        &mut self,
        alias: &str,
        mut args: Vec<Value>,
        call: impl FnOnce(&mut Interpreter, Vec<Value>) -> BasicResult<T>,
    ) -> BasicResult<T> {
        let mut module = self
            .imported_modules
            .remove(alias)
            .expect("validated import alias");
        let mut borrowed_arrays = Vec::<(String, String)>::new();
        for arg in &mut args {
            let Value::ArrayRef(key) = arg else { continue };
            if let Some((_, slot)) = borrowed_arrays.iter().find(|(source, _)| source == key) {
                *key = slot.clone();
                continue;
            }
            // Unspellable names keep passed arrays accessible through their
            // parameters alone. REDIM determines element type from the key.
            let suffix = if key.ends_with('$') { "$" } else { "" };
            let slot = format!("\0IMPORT{}{suffix}", borrowed_arrays.len());
            if let Some(array) = self.arrays.remove(key) {
                module.state.arrays.insert(slot.clone(), array);
            }
            borrowed_arrays.push((std::mem::replace(key, slot.clone()), slot));
        }

        self.active_module = Some(ActiveModule {
            path: module.path.clone(),
            function_base: self.active_functions.len(),
            sub_base: self.active_subs.len(),
            gosub_depth: self.gosub_stack.len(),
        });
        module.state.swap(self);
        let result = call(self, args).map_err(|error| self.import_runtime_error(error));
        self.pause_deadline = None;
        module.state.swap(self);
        self.active_module = None;

        for (source, slot) in borrowed_arrays {
            if let Some(array) = module.state.arrays.remove(&slot) {
                self.arrays.insert(source, array);
            }
        }
        self.imported_modules.insert(alias.to_string(), module);
        result
    }

    fn import_runtime_error(&self, error: BasicError) -> BasicError {
        if matches!(
            error.detail.as_deref(),
            Some(DEBUG_ABORT_DETAIL | DEBUG_RESTART_DETAIL)
        ) {
            return error;
        }
        let path = &self.active_module.as_ref().unwrap().path;
        let mut error = self.with_current_line(error);
        error.source_path = Some(path.clone());
        error
    }
}
