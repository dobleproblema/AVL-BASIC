//! Statistical wall-time profiling for the immediate PROFILE command.
use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

const RUNTIME: usize = 0;
const FRAME_WAIT: usize = 1;
const FINISHED: usize = usize::MAX;

pub(crate) trait FrameWait: Copy {
    fn during<T>(self, operation: impl FnOnce() -> T) -> T;
}

impl FrameWait for () {
    #[inline(always)]
    fn during<T>(self, operation: impl FnOnce() -> T) -> T {
        operation()
    }
}

impl FrameWait for &AtomicUsize {
    fn during<T>(self, operation: impl FnOnce() -> T) -> T {
        let line = self.load(Ordering::Relaxed);
        self.store(FRAME_WAIT, Ordering::Relaxed);
        let result = operation();
        self.store(line, Ordering::Relaxed);
        result
    }
}

type Samples = HashMap<usize, u64>;

struct Sampler {
    marker: Arc<AtomicUsize>,
    worker: Option<JoinHandle<Samples>>,
    start: Instant,
}

impl Sampler {
    fn start() -> io::Result<Self> {
        let marker = Arc::new(AtomicUsize::new(RUNTIME));
        let shared = marker.clone();
        let start = Instant::now();
        let worker = thread::Builder::new()
            .name("AVL PROFILE".into())
            .spawn(move || {
                let mut samples = Samples::new();
                let mut random = 0x9e37_79b9_u32;
                loop {
                    // Vary the interval so periodic drawing and BASIC timers are not sampled in lockstep.
                    random ^= random << 13;
                    random ^= random >> 17;
                    random ^= random << 5;
                    // On Windows sleep uses a high-resolution waitable timer. A timed park
                    // can round every sample to the same 15.6 ms scheduler tick.
                    thread::sleep(Duration::from_micros(3000 + u64::from(random % 4001)));
                    let id = shared.load(Ordering::Relaxed);
                    if id == FINISHED {
                        break;
                    }
                    *samples.entry(id).or_default() += 1;
                }
                samples
            })?;
        Ok(Self {
            marker,
            worker: Some(worker),
            start,
        })
    }

    fn stop(&mut self) -> thread::Result<Samples> {
        self.marker.store(FINISHED, Ordering::Relaxed);
        let worker = self.worker.take().expect("sampler joined once");
        worker.join()
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.stop();
        }
    }
}

#[derive(Debug)]
struct Row {
    line: i32,
    text: String,
    routine: String,
    calls: Cell<u64>,
}
#[derive(Debug)]
struct Catalog {
    base: usize,
    numbers: Rc<[i32]>,
    commands: Rc<[Rc<[Rc<CachedCommand>]>]>,
    name: String,
    identifier_case: HashMap<String, String>,
    rows: Vec<Row>,
    visits: Vec<Cell<u64>>,
    starts: Vec<bool>,
}

#[derive(Debug)]
pub(super) struct Session {
    marker: Arc<AtomicUsize>,
    catalogs: RefCell<HashMap<*const i32, Rc<Catalog>>>,
    current_catalog: RefCell<Option<Rc<Catalog>>>,
    next_id: Cell<usize>,
}

impl Session {
    fn catalog(&self, interpreter: &Interpreter) -> Rc<Catalog> {
        let numbers = &interpreter.line_numbers_cache;
        if let Some(catalog) = self.current_catalog.borrow().as_ref() {
            if Rc::ptr_eq(numbers, &catalog.numbers) {
                return catalog.clone();
            }
        }
        let key = numbers.as_ptr();
        if let Some(catalog) = self.catalogs.borrow().get(&key) {
            self.current_catalog.replace(Some(catalog.clone()));
            return catalog.clone();
        }
        let name = if let Some(module) = &interpreter.active_module {
            module.path.display().to_string()
        } else if interpreter.program_transfer_generation == 0 {
            "main".into()
        } else {
            format!("main#{}", interpreter.program_transfer_generation)
        };
        let rows = numbers
            .iter()
            .map(|&line| {
                let routine = if let Some(name) = interpreter.function_for_line(line) {
                    name.to_owned()
                } else if let Some(name) = interpreter.sub_for_line(line) {
                    format!("SUB {name}")
                } else {
                    "(main)".into()
                };
                let code = interpreter.program.get(line).unwrap_or("");
                Row {
                    line,
                    text: code.strip_prefix(' ').unwrap_or(code).to_owned(),
                    routine,
                    calls: Cell::new(0),
                }
            })
            .collect();
        let mut starts = vec![false; numbers.len()];
        for sub in interpreter.subs.values() {
            if let Some(entry) = starts.get_mut(sub.start.line_idx) {
                *entry = true;
            }
        }
        for function in interpreter.functions.values() {
            if let UserFunction::Multi { start, .. } = function {
                if let Some(entry) = starts.get_mut(start.line_idx) {
                    *entry = true;
                }
            }
        }
        let base = self.next_id.get();
        self.next_id.set(base + numbers.len());
        let catalog = Rc::new(Catalog {
            base,
            numbers: numbers.clone(),
            commands: interpreter.compiled_line_cache.clone(),
            identifier_case: interpreter.identifier_case.clone(),
            name,
            rows,
            visits: vec![Cell::new(0); numbers.len()],
            starts,
        });
        self.catalogs.borrow_mut().insert(key, catalog.clone());
        self.current_catalog.replace(Some(catalog.clone()));
        catalog
    }

    fn report(&self, samples: &Samples, elapsed: Duration, ansi: bool, stream: bool) -> String {
        use console::profile::{Column, Report};

        let total: u64 = samples.values().sum();
        let seconds = elapsed.as_secs_f64();
        let estimates = |hits: u64| {
            if hits == 0 {
                return ("-".to_owned(), "-".to_owned());
            }
            let share = hits as f64 / total as f64;
            (
                format!("{:.1}", seconds * 1000.0 * share),
                format!("{:.1}", 100.0 * share),
            )
        };

        let catalogs = self.catalogs.borrow();
        let mut lines = Vec::new();
        let mut routines: BTreeMap<(&str, &str), (u64, u64, u64)> = BTreeMap::new();
        for table in catalogs.values() {
            for (index, row) in table.rows.iter().enumerate() {
                let visits = table.visits[index].get();
                if visits == 0 {
                    continue;
                }
                let hits = samples.get(&(table.base + index)).copied().unwrap_or(0);
                lines.push((hits, visits, table.as_ref(), row));
                let r = routines.entry((&table.name, &row.routine)).or_default();
                r.0 += hits;
                r.1 += visits;
                r.2 += row.calls.get();
            }
        }
        let mut modules: Vec<_> = routines.keys().map(|&(name, _)| name).collect();
        modules.dedup(); // Routines are already ordered by module, then name.
        modules.sort_unstable_by_key(|&name| (name != "main", name));
        let show_modules = modules.iter().any(|&name| name != "main");
        let ids: BTreeMap<_, _> = modules
            .iter()
            .enumerate()
            .map(|(i, &name)| (name, (i + 1).to_string()))
            .collect();
        let mut report = Report::new(ansi, stream, show_modules);
        let hz = if seconds == 0.0 {
            0.0
        } else {
            total as f64 / seconds
        };
        report.heading(
            "PROFILE",
            &format!("  elapsed {seconds:.3} s  samples {total}  ({hz:.0} Hz)"),
        );

        let outside =
            [(RUNTIME, "Runtime / setup"), (FRAME_WAIT, "FRAME wait")].map(|(id, label)| {
                let hits = samples.get(&id).copied().unwrap_or(0);
                let (time, share) = estimates(hits);
                [label.to_owned(), hits.to_string(), time, share]
            });
        let samples_col = ("Samples", Column::Number);
        let time_col = ("Est. ms", Column::Number);
        let share_col = ("%", Column::Number);
        let module_col = ("Mod", Column::ModuleId);
        let visits_col = ("Visits", Column::Number);
        report.table(
            [
                ("Outside BASIC", Column::Text),
                samples_col,
                time_col,
                share_col,
            ],
            outside.into(),
        );

        if show_modules {
            report.heading("MODULES", "");
            report.table(
                [module_col, ("Source", Column::Text)],
                modules
                    .iter()
                    .map(|&name| [ids[name].clone(), name.to_owned()])
                    .collect(),
            );
        }
        let mut routines: Vec<_> = routines.into_iter().collect();
        routines.sort_by_key(|(name, (hits, _, _))| (std::cmp::Reverse(*hits), *name));
        report.heading("ROUTINES", "");
        report.table(
            [
                module_col,
                ("Calls", Column::Number),
                visits_col,
                samples_col,
                time_col,
                share_col,
                ("Routine", Column::Routine),
            ],
            routines
                .into_iter()
                .map(|((module, name), (hits, visits, calls))| {
                    let (time, share) = estimates(hits);
                    [
                        ids[module].clone(),
                        calls.to_string(),
                        visits.to_string(),
                        hits.to_string(),
                        time,
                        share,
                        name.to_owned(),
                    ]
                })
                .collect(),
        );

        lines.sort_by_key(|(hits, _, table, row)| {
            (
                std::cmp::Reverse(*hits),
                table.name.as_str(),
                row.line,
                table.base,
            )
        });
        report.heading("LINES", "");
        report.table(
            [
                module_col,
                ("Line", Column::Line),
                visits_col,
                samples_col,
                time_col,
                share_col,
                ("Source", Column::Source),
            ],
            lines
                .into_iter()
                .map(|(hits, visits, table, row)| {
                    let (time, share) = estimates(hits);
                    let source = apply_identifier_case(&row.text, &table.identifier_case);
                    [
                        ids[table.name.as_str()].clone(),
                        row.line.to_string(),
                        visits.to_string(),
                        hits.to_string(),
                        time,
                        share,
                        source,
                    ]
                })
                .collect(),
        );
        report.finish()
    }
}

pub(super) trait Observer {
    fn visit(&self, _index: usize, _retained: Option<&RetainedLinePlan>) {}
    fn set_catalog(&mut self, _interpreter: &Interpreter) {}
    #[inline(always)]
    fn execute(
        &self,
        interpreter: &mut Interpreter,
        command: &CachedCommand,
        cursor: &mut Cursor,
    ) -> BasicResult<()> {
        interpreter.execute_cached_command(command, cursor, &[])
    }
}
impl Observer for () {}

struct Scope {
    session: Rc<Session>,
    table: Rc<Catalog>,
    previous: usize,
}

impl Scope {
    fn enter(session: Rc<Session>, interpreter: &Interpreter, cursor: Cursor) -> Self {
        let previous = session.marker.load(Ordering::Relaxed);
        let table = session.catalog(interpreter);
        if interpreter.run_depth > 1
            && cursor.cmd_idx == 0
            && table.starts.get(cursor.line_idx) == Some(&true)
        {
            let calls = &table.rows[cursor.line_idx].calls;
            calls.set(calls.get() + 1);
        }
        Self {
            session,
            table,
            previous,
        }
    }

    #[inline]
    fn record(&self, table: &Catalog, index: usize) {
        let visits = &table.visits[index];
        visits.set(visits.get() + 1);
        self.session
            .marker
            .store(table.base + index, Ordering::Relaxed);
    }

    // MERGE can leave a return into the old compiled line. Keep its original source and row.
    #[cold]
    fn retained(&self, plan: &RetainedLinePlan) {
        for table in self.session.catalogs.borrow().values() {
            if let Ok(index) = table.numbers.binary_search(&plan.line) {
                if Rc::ptr_eq(&plan.commands, &table.commands[index]) {
                    self.record(table, index);
                    return;
                }
            }
        }
    }
}

impl Observer for Scope {
    #[inline]
    fn visit(&self, index: usize, retained: Option<&RetainedLinePlan>) {
        if let Some(plan) = retained {
            self.retained(plan);
        } else {
            self.record(&self.table, index);
        }
    }
    fn set_catalog(&mut self, interpreter: &Interpreter) {
        self.table = self.session.catalog(interpreter);
    }

    #[inline]
    fn execute(
        &self,
        interpreter: &mut Interpreter,
        command: &CachedCommand,
        cursor: &mut Cursor,
    ) -> BasicResult<()> {
        if let CachedCommand::Raw(text) = command {
            if first_word_is(text, "FRAME") {
                return interpreter.execute_frame(
                    text[5..].trim(),
                    cursor,
                    self.session.marker.as_ref(),
                );
            }
        }
        interpreter.execute_cached_command(command, cursor, &[])
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        self.session.marker.store(self.previous, Ordering::Relaxed);
    }
}

impl Interpreter {
    #[inline(never)]
    pub(super) fn run_with_observer(&mut self, cursor: Cursor) -> BasicResult<RunOutcome> {
        match self
            .debugger_state
            .as_ref()
            .and_then(|state| state.observer.as_ref())
            .expect("active execution observer")
        {
            ExecutionObserver::Debug(_) => self.run_from_inner_debug(cursor),
            ExecutionObserver::Profile(session) => {
                let scope = Scope::enter(session.clone(), self, cursor);
                self.run_loop(cursor, scope)
            }
        }
    }

    pub(super) fn execute_profile(&mut self) -> BasicResult<()> {
        let mut sampler = Sampler::start().map_err(|error| {
            self.err(ErrorCode::Unsupported)
                .with_detail(error.to_string())
        })?;
        let session = Rc::new(Session {
            marker: sampler.marker.clone(),
            catalogs: RefCell::default(),
            current_catalog: RefCell::default(),
            next_id: Cell::new(2),
        });
        let previous = self
            .debugger_state
            .get_or_insert_with(Default::default)
            .observer
            .replace(ExecutionObserver::Profile(session.clone()));
        let result = self.execute_immediate_run("");
        if let Some(state) = self.debugger_state.as_mut() {
            state.observer = previous;
            if state.editor_breakpoints.is_empty() && state.observer.is_none() {
                self.debugger_state = None;
            }
        }
        let elapsed = sampler.start.elapsed();
        let samples = sampler.stop().map_err(|_| {
            self.err(ErrorCode::Unsupported)
                .with_detail("PROFILE sampling thread stopped unexpectedly.")
        })?;
        self.finish_output_line();
        for line in session
            .report(&samples, elapsed, self.ansi_output, self.stream_output)
            .lines()
        {
            self.write_line(line);
        }
        result
    }
}
