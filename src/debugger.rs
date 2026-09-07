use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::io;
use std::ops::Range;
use std::time::Duration;

/// Action selected while program execution is synchronously paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    Continue,
    StepInto,
    StepOver,
    StepOut,
    Restart,
    Abort,
}

/// Reason why execution entered the debugger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugPauseReason {
    Breakpoint,
    Step,
    PauseRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugLocation {
    pub line: i32,
    /// Zero-based statement index within the BASIC source line.
    pub statement: usize,
    /// Byte range of the statement within `source`.
    ///
    /// The range is absolute within the complete numbered source line. The
    /// debugger UI converts its start to a character column before rendering,
    /// so non-ASCII text before the statement does not shift the marker.
    pub source_span: Option<Range<usize>>,
    pub source: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DebugValue {
    Number(f64),
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugVariable {
    pub name: String,
    pub value: DebugValue,
}

/// Position of the value that the next READ will consume.
#[derive(Debug, Clone, PartialEq)]
pub enum DebugDataSnapshot {
    /// The program contains no DATA values.
    Empty,
    Next {
        /// Zero-based position in the complete DATA stream.
        position: usize,
        line: i32,
        /// One-based position among all DATA values on `line`.
        line_item: usize,
        value: DebugValue,
    },
    Exhausted {
        position: usize,
        total: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugArrayKind {
    Number,
    String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugArraySummary {
    pub name: String,
    /// Visible target name when this array is a parameter alias.
    pub alias_of: Option<String>,
    pub kind: DebugArrayKind,
    /// BASIC upper bounds, matching the values used by DIM.
    pub dimensions: Vec<usize>,
    pub elements: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugFrameKind {
    Program,
    Gosub,
    Sub,
    Function,
    Timer,
    Mouse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugStackFrame {
    pub kind: DebugFrameKind,
    pub name: Option<String>,
    pub line: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugTimerSnapshot {
    pub number: i32,
    pub target: i32,
    pub repeat: bool,
    pub active: bool,
    pub interval: Duration,
    pub remaining: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugSnapshot {
    pub reason: DebugPauseReason,
    pub location: DebugLocation,
    /// Current program buffer, rebuilt only when execution pauses.
    pub source_lines: Vec<String>,
    pub variables: Vec<DebugVariable>,
    /// Last successfully written scalar element of each physical array.
    pub array_elements: Vec<DebugVariable>,
    pub arrays: Vec<DebugArraySummary>,
    pub stack: Vec<DebugStackFrame>,
    pub err: i32,
    pub erl: i32,
    pub timers: Vec<DebugTimerSnapshot>,
    pub data: DebugDataSnapshot,
}

/// A visible debugger value, with concrete array indices rather than expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugEditTarget {
    Scalar(String),
    ArrayElement { name: String, indexes: Vec<i32> },
}

/// A complete statement in the numbered source line (UTF-8 byte offsets).
/// Inline THEN/ELSE children are deliberately not navigation targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugStatementTarget {
    pub line: i32,
    pub source_span: Range<usize>,
}

/// Restricted access to a live interpreter during one synchronous debugger pause.
/// Editing a value never evaluates BASIC or resumes the program. Validation
/// errors are returned to the debugger UI, without entering BASIC error handling.
pub trait DebugPauseAccess {
    fn idle(&mut self) -> io::Result<()>;
    fn set_variable(
        &mut self,
        target: &DebugEditTarget,
        value: DebugValue,
    ) -> Result<DebugSnapshot, String>;

    fn set_scalar(&mut self, name: &str, value: DebugValue) -> Result<DebugSnapshot, String> {
        self.set_variable(&DebugEditTarget::Scalar(name.to_string()), value)
    }

    fn statement_targets(&self) -> Vec<DebugStatementTarget> {
        Vec::new()
    }

    fn set_next(&mut self, _target: &DebugStatementTarget) -> Result<DebugSnapshot, String> {
        Err("Changing the next statement is not supported".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DebugStep {
    Into,
    Over { depth: usize },
    Out { depth: usize },
}

enum DebugPauseHandler {
    Interactive(
        Box<
            dyn FnMut(
                &DebugSnapshot,
                &mut HashSet<i32>,
                &mut dyn FnMut() -> io::Result<()>,
            ) -> io::Result<DebugAction>,
        >,
    ),
    InteractiveEditable(
        Box<
            dyn FnMut(
                &DebugSnapshot,
                &mut HashSet<i32>,
                &mut dyn DebugPauseAccess,
            ) -> io::Result<DebugAction>,
        >,
    ),
    Scripted {
        actions: VecDeque<DebugAction>,
        snapshots: Vec<DebugSnapshot>,
    },
}

/// Runtime-independent debugger state.
///
/// The handler is called synchronously. It must return only when execution is
/// meant to resume, which keeps Rust and BASIC call frames alive while paused.
pub struct Debugger {
    breakpoints: HashSet<i32>,
    pause_requested: bool,
    pub(crate) step: Option<DebugStep>,
    handler: DebugPauseHandler,
}

impl fmt::Debug for Debugger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let handler = match &self.handler {
            DebugPauseHandler::Interactive(_) => "interactive",
            DebugPauseHandler::InteractiveEditable(_) => "interactive_editable",
            DebugPauseHandler::Scripted { .. } => "scripted",
        };
        f.debug_struct("Debugger")
            .field("breakpoints", &self.breakpoints)
            .field("pause_requested", &self.pause_requested)
            .field("step", &self.step)
            .field("handler", &handler)
            .finish()
    }
}

impl Debugger {
    pub fn interactive(
        handler: impl FnMut(
                &DebugSnapshot,
                &mut HashSet<i32>,
                &mut dyn FnMut() -> io::Result<()>,
            ) -> io::Result<DebugAction>
            + 'static,
    ) -> Self {
        Self {
            breakpoints: HashSet::new(),
            pause_requested: false,
            step: None,
            handler: DebugPauseHandler::Interactive(Box::new(handler)),
        }
    }

    /// Creates a synchronous pause handler that can edit existing visible values.
    /// The access object is only valid during the callback; returning an action
    /// is still the only way to resume or abort execution.
    pub fn interactive_editable(
        handler: impl FnMut(
                &DebugSnapshot,
                &mut HashSet<i32>,
                &mut dyn DebugPauseAccess,
            ) -> io::Result<DebugAction>
            + 'static,
    ) -> Self {
        Self {
            breakpoints: HashSet::new(),
            pause_requested: false,
            step: None,
            handler: DebugPauseHandler::InteractiveEditable(Box::new(handler)),
        }
    }

    /// Creates a deterministic headless debugger. Every pause records a
    /// snapshot and consumes one action. Exhausted scripts abort execution.
    pub fn scripted(actions: impl IntoIterator<Item = DebugAction>) -> Self {
        Self {
            breakpoints: HashSet::new(),
            pause_requested: false,
            step: None,
            handler: DebugPauseHandler::Scripted {
                actions: actions.into_iter().collect(),
                snapshots: Vec::new(),
            },
        }
    }

    pub fn set_breakpoint(&mut self, line: i32, enabled: bool) {
        if enabled {
            self.breakpoints.insert(line);
        } else {
            self.breakpoints.remove(&line);
        }
    }

    pub fn toggle_breakpoint(&mut self, line: i32) -> bool {
        if self.breakpoints.remove(&line) {
            false
        } else {
            self.breakpoints.insert(line);
            true
        }
    }

    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    pub fn replace_breakpoints(&mut self, breakpoints: impl IntoIterator<Item = i32>) {
        self.breakpoints = breakpoints.into_iter().collect();
    }

    pub fn breakpoints(&self) -> impl Iterator<Item = i32> + '_ {
        self.breakpoints.iter().copied()
    }

    pub fn request_pause(&mut self) {
        self.pause_requested = true;
    }

    pub fn snapshots(&self) -> &[DebugSnapshot] {
        match &self.handler {
            DebugPauseHandler::Scripted { snapshots, .. } => snapshots,
            DebugPauseHandler::Interactive(_) | DebugPauseHandler::InteractiveEditable(_) => &[],
        }
    }

    pub fn take_snapshots(&mut self) -> Vec<DebugSnapshot> {
        match &mut self.handler {
            DebugPauseHandler::Scripted { snapshots, .. } => std::mem::take(snapshots),
            DebugPauseHandler::Interactive(_) | DebugPauseHandler::InteractiveEditable(_) => {
                Vec::new()
            }
        }
    }

    pub(crate) fn is_breakpoint(&self, line: i32) -> bool {
        self.breakpoints.contains(&line)
    }

    pub(crate) fn take_pause_request(&mut self) -> bool {
        std::mem::take(&mut self.pause_requested)
    }

    pub(crate) fn pause(
        &mut self,
        snapshot: &DebugSnapshot,
        access: &mut dyn DebugPauseAccess,
    ) -> io::Result<DebugAction> {
        match &mut self.handler {
            DebugPauseHandler::Interactive(handler) => {
                handler(snapshot, &mut self.breakpoints, &mut || access.idle())
            }
            DebugPauseHandler::InteractiveEditable(handler) => {
                handler(snapshot, &mut self.breakpoints, access)
            }
            DebugPauseHandler::Scripted { actions, snapshots } => {
                snapshots.push(snapshot.clone());
                Ok(actions.pop_front().unwrap_or(DebugAction::Abort))
            }
        }
    }

    pub(crate) fn arm_step(&mut self, action: DebugAction, depth: usize) {
        self.step = match action {
            DebugAction::Continue | DebugAction::Restart | DebugAction::Abort => None,
            DebugAction::StepInto => Some(DebugStep::Into),
            DebugAction::StepOver => Some(DebugStep::Over { depth }),
            DebugAction::StepOut => Some(DebugStep::Out { depth }),
        };
    }

    pub(crate) fn step_is_due(&self, depth: usize) -> bool {
        match self.step {
            None => false,
            Some(DebugStep::Into) => true,
            Some(DebugStep::Over { depth: origin }) => depth <= origin,
            Some(DebugStep::Out { depth: origin }) => depth < origin,
        }
    }

    pub(crate) fn consume_step(&mut self) {
        self.step = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoEdits;

    impl DebugPauseAccess for NoEdits {
        fn idle(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn set_variable(
            &mut self,
            _: &DebugEditTarget,
            _: DebugValue,
        ) -> Result<DebugSnapshot, String> {
            Err("Not editable".into())
        }
    }

    #[test]
    fn scripted_debugger_records_snapshots_and_defaults_to_abort() {
        let mut debugger = Debugger::scripted([DebugAction::Continue]);
        let snapshot = DebugSnapshot {
            reason: DebugPauseReason::Breakpoint,
            location: DebugLocation {
                line: 10,
                statement: 0,
                source_span: None,
                source: " A=1".to_string(),
                command: "A=1".to_string(),
            },
            source_lines: vec!["10 A=1".to_string()],
            variables: Vec::new(),
            array_elements: Vec::new(),
            arrays: Vec::new(),
            stack: Vec::new(),
            err: 0,
            erl: 0,
            timers: Vec::new(),
            data: DebugDataSnapshot::Empty,
        };

        let mut idle = NoEdits;
        assert_eq!(
            debugger.pause(&snapshot, &mut idle).unwrap(),
            DebugAction::Continue
        );
        assert_eq!(
            debugger.pause(&snapshot, &mut idle).unwrap(),
            DebugAction::Abort
        );
        assert_eq!(debugger.snapshots(), &[snapshot.clone(), snapshot]);
    }

    #[test]
    fn step_depth_rules_match_into_over_and_out() {
        let mut debugger = Debugger::scripted([]);
        debugger.arm_step(DebugAction::StepInto, 4);
        assert!(debugger.step_is_due(99));
        debugger.arm_step(DebugAction::StepOver, 4);
        assert!(!debugger.step_is_due(5));
        assert!(debugger.step_is_due(4));
        debugger.arm_step(DebugAction::StepOut, 4);
        assert!(!debugger.step_is_due(4));
        assert!(debugger.step_is_due(3));
    }
}
