//! BASIC audio syntax and event dispatch. The audio thread never executes BASIC.
use super::*;
use crate::audio::cpc::Section;

impl Interpreter {
    pub(super) fn reset_audio(&mut self) {
        self.audio.reset();
        self.sound_handlers = [None; 3];
        self.has_sound_handlers = false;
        self.sound_isr_markers.clear();
        self.pending_sounds.clear();
    }

    fn audio_integer(&mut self, text: &str, min: i32, max: i32) -> BasicResult<i32> {
        let value = self.eval_number(text)?;
        self.audio_integer_value(value, min, max)
    }

    fn audio_integer_value(&self, value: f64, min: i32, max: i32) -> BasicResult<i32> {
        if !value.is_finite() || value.fract() != 0.0 || value < min as f64 || value > max as f64 {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        Ok(value as i32)
    }

    fn cpc_integer(&mut self, text: &str, min: i32, max: i32) -> BasicResult<i32> {
        let value = self.eval_number(text)?;
        self.cpc_integer_value(value, min, max)
    }

    fn cpc_integer_value(&self, value: f64, min: i32, max: i32) -> BasicResult<i32> {
        // Locomotive BASIC converts real expressions to the nearest integer;
        // exact halves round away from zero. Check the converted range before
        // casting, so octave divisions work without hiding numeric overflow.
        self.audio_integer_value(value.round(), min, max)
    }

    fn audio_number(&mut self, text: &str, min: f64, max: f64) -> BasicResult<f64> {
        let value = self.eval_number(text)?;
        if !value.is_finite() || !(min..=max).contains(&value) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        Ok(value)
    }

    fn sound_channel(&self, value: f64) -> BasicResult<u8> {
        let channel = self.cpc_integer_value(value, 1, 4)? as u8;
        if !matches!(channel, 1 | 2 | 4) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        Ok(channel)
    }

    pub(super) fn execute_sound(&mut self, text: &str, cursor: &mut Cursor) -> BasicResult<()> {
        let origin = *cursor;
        let pending = self.pending_sounds.iter().position(|(at, _)| *at == origin);
        let note = if let Some(index) = pending {
            self.pending_sounds.remove(index).1
        } else {
            let args = split_arguments(text);
            if !(2..=7).contains(&args.len()) || args[..2].iter().any(|s| s.trim().is_empty()) {
                return Err(self.err(ErrorCode::ArgumentMismatch));
            }
            let mut values = [0, 0, 20, 12, 0, 0, 0];
            let limits = [
                (1, 255),
                (0, 4095),
                (-32768, 32767),
                (0, 15),
                (0, 15),
                (0, 15),
                (0, 31),
            ];
            for (index, arg) in args.iter().enumerate() {
                if !arg.trim().is_empty() {
                    values[index] = self.cpc_integer(arg, limits[index].0, limits[index].1)?;
                }
            }
            if values[0] & 7 == 0 {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            SoundNote {
                state: values[0] as u8,
                period: values[1] as u16,
                duration: values[2] as i16,
                volume: values[3] as u8,
                volume_env: values[4] as u8,
                tone_env: values[5] as u8,
                noise: values[6] as u8,
            }
        };
        loop {
            if self.audio.enqueue(note) {
                return Ok(());
            }
            // Retry the evaluated note after a GOSUB interrupt, not its expressions.
            self.check_user_interrupt(cursor)?;
            if self.process_timers(cursor)? {
                self.pending_sounds.push((origin, note));
                return Ok(());
            }
            if self.end_requested || self.stopped_cursor.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub(super) fn execute_sound_envelope(&mut self, text: &str, tone: bool) -> BasicResult<()> {
        let args = split_arguments(text);
        if args.is_empty() || args[0].trim().is_empty() {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let number = self.cpc_integer(&args[0], if tone { -15 } else { 1 }, 15)?;
        if number == 0 {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        let mut sections = Vec::with_capacity(5);
        let mut index = 1;
        while index < args.len() {
            if sections.len() == 5 {
                return Err(self.err(ErrorCode::InvalidArgument));
            }
            if let Some(absolute) = args[index].trim().strip_prefix('=') {
                if !tone {
                    return Err(self.err(ErrorCode::Unsupported).with_detail(
                        "AY hardware envelope sections are not supported; use ENV steps.",
                    ));
                }
                if index + 1 >= args.len() {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                let value = self.cpc_integer(absolute, 0, 4095)? as u16;
                let ticks = self.cpc_integer(&args[index + 1], 0, 255)? as u16;
                sections.push(Section::Absolute {
                    value,
                    ticks: if ticks == 0 { 256 } else { ticks },
                });
                index += 2;
            } else {
                if index + 2 >= args.len() {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                let steps = self.cpc_integer(&args[index], 0, if tone { 239 } else { 127 })? as u8;
                let delta = self.cpc_integer(&args[index + 1], -128, 127)? as i16;
                let ticks = self.cpc_integer(&args[index + 2], 0, 255)? as u16;
                sections.push(Section::Step {
                    steps,
                    delta,
                    ticks: if ticks == 0 { 256 } else { ticks },
                });
                index += 3;
            }
        }
        if tone {
            self.audio
                .define_tone(number.unsigned_abs() as u8, number < 0, sections);
        } else {
            self.audio.define_volume(number as u8, sections);
        }
        Ok(())
    }

    pub(super) fn execute_sound_release(&mut self, text: &str) -> BasicResult<()> {
        let args = split_arguments(text);
        if args.len() != 1 || args[0].trim().is_empty() {
            return Err(self.err(ErrorCode::ArgumentMismatch));
        }
        let mask = self.cpc_integer(&args[0], 1, 7)? as u8;
        self.audio.release(mask);
        Ok(())
    }

    pub(super) fn execute_on_sq(&mut self, command: &str) -> BasicResult<()> {
        if self.current_line.is_none() {
            return Err(self.err(ErrorCode::NonImmediateCommand));
        }
        let body = command[5..].trim();
        if !body.starts_with('(') {
            return Err(self.err(ErrorCode::Syntax));
        }
        let mut depth = 0;
        let mut in_string = false;
        let close = body.char_indices().find_map(|(index, ch)| {
            if ch == '"' {
                in_string = !in_string;
            } else if !in_string {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(index);
                        }
                    }
                    _ => {}
                }
            }
            None
        });
        let Some(close) = close else {
            return Err(self.err(ErrorCode::Syntax));
        };
        let tail = body[close + 1..].trim();
        if !starts_keyword(&tail.to_ascii_uppercase(), "GOSUB") {
            return Err(self.err(ErrorCode::Syntax));
        }
        let value = self.eval_number(&body[1..close])?;
        let channel = self.sound_channel(value)?;
        let target = parse_line_number_literal(tail[5..].trim())
            .ok_or_else(|| self.err(ErrorCode::InvalidLineNumber))?;
        if target != 0 {
            let at = self
                .cursor_for_line(target)
                .ok_or_else(|| self.err(ErrorCode::TargetLineNotFound))?;
            self.validate_function_jump(target, &at, true)?;
        }
        self.sound_handlers[channel.trailing_zeros() as usize] = (target != 0).then_some(target);
        self.has_sound_handlers = self.sound_handlers.iter().any(Option::is_some);
        Ok(())
    }

    pub(super) fn process_sound_events(&mut self, cursor: &mut Cursor) -> BasicResult<bool> {
        if !self.has_sound_handlers
            || !self.sound_isr_markers.is_empty()
            || self.current_interrupt_priority >= 0
            || !self.interrupts_enabled
        {
            return Ok(false);
        }
        for index in 0..3 {
            let Some(target) = self.sound_handlers[index] else {
                continue;
            };
            if self.audio.sq(1 << index) & 7 == 0 {
                continue;
            }
            let at = self
                .cursor_for_line(target)
                .ok_or_else(|| self.err(ErrorCode::TargetLineNotFound))?;
            self.validate_function_jump(target, &at, true)?;
            self.sound_handlers[index] = None;
            self.has_sound_handlers = self.sound_handlers.iter().any(Option::is_some);
            self.sound_isr_markers
                .push((self.gosub_stack.len(), self.interrupts_enabled));
            self.push_gosub_return(*cursor);
            *cursor = at;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn execute_audio(&mut self, text: &str) -> BasicResult<()> {
        let text = text.trim();
        let (verb, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        let verb = verb.to_ascii_uppercase();
        let args = if rest.trim().is_empty() {
            Vec::new()
        } else {
            split_arguments(rest)
        };
        match verb.as_str() {
            "ON" | "OFF" => {
                if !args.is_empty() {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                self.audio.set_enabled(verb == "ON");
            }
            "LOAD" => {
                if args.len() != 2 {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                let id = self.audio_integer(&args[0], 1, 64)? as u8;
                let Value::Str(path) = self.eval_value(&args[1])? else {
                    return Err(self.err(ErrorCode::TypeMismatch));
                };
                if path.trim().is_empty() || path.contains('\0') {
                    return Err(self.err(ErrorCode::InvalidArgument));
                }
                let path = Self::match_existing_case(
                    self.resolve_virtual_path_text(&path, &self.effective_base_dir())?,
                );
                let metadata = fs::metadata(&path).map_err(|error| {
                    self.err(if error.kind() == io::ErrorKind::NotFound {
                        ErrorCode::FileNotFound
                    } else {
                        ErrorCode::FileIoError
                    })
                    .with_detail(error.to_string())
                })?;
                if !metadata.is_file() {
                    return Err(self.err(ErrorCode::InvalidFileData));
                }
                self.audio
                    .load(id, &path)
                    .map_err(|error| self.err(ErrorCode::InvalidFileData).with_detail(error))?;
            }
            "PLAY" => {
                if !(2..=6).contains(&args.len()) {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                let channel = self.audio_integer(&args[0], 1, 32)? as u8;
                let id = self.audio_integer(&args[1], 1, 64)? as u8;
                let mut looping = false;
                let mut volume = 1.0;
                let mut pan = 0.0;
                let mut rate = 1.0;
                if let Some(arg) = args.get(2).filter(|s| !s.trim().is_empty()) {
                    looping = self.audio_integer(arg, -1, 1)? != 0;
                }
                if let Some(arg) = args.get(3).filter(|s| !s.trim().is_empty()) {
                    volume = self.audio_number(arg, 0.0, 1.0)?;
                }
                if let Some(arg) = args.get(4).filter(|s| !s.trim().is_empty()) {
                    pan = self.audio_number(arg, -1.0, 1.0)?;
                }
                if let Some(arg) = args.get(5).filter(|s| !s.trim().is_empty()) {
                    rate = self.audio_number(arg, 0.125, 8.0)?;
                }
                self.audio
                    .play(channel, id, looping, volume, pan, rate)
                    .map_err(|error| self.err(ErrorCode::InvalidArgument).with_detail(error))?;
            }
            "UNLOAD" | "STOP" | "PAUSE" | "RESUME" => {
                if args.len() > 1 {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                let channel = args
                    .first()
                    .map(|arg| {
                        self.audio_integer(arg, 1, if verb == "UNLOAD" { 64 } else { 32 })
                            .map(|n| n as u8)
                    })
                    .transpose()?;
                match verb.as_str() {
                    "UNLOAD" => self.audio.unload(channel),
                    "STOP" => self.audio.stop(channel),
                    "PAUSE" => self.audio.pause(channel),
                    _ => self.audio.resume(channel),
                }
            }
            "VOLUME" | "PAN" | "RATE" => {
                if !(2..=3).contains(&args.len()) {
                    return Err(self.err(ErrorCode::ArgumentMismatch));
                }
                let channel = self.audio_integer(&args[0], 1, 32)? as u8;
                let (min, max) = match verb.as_str() {
                    "VOLUME" => (0.0, 1.0),
                    "PAN" => (-1.0, 1.0),
                    _ => (0.125, 8.0),
                };
                let value = self.audio_number(&args[1], min, max)?;
                let fade = args
                    .get(2)
                    .map(|s| self.audio_integer(s, 0, 600_000))
                    .transpose()?
                    .unwrap_or(0) as u64;
                let result = match verb.as_str() {
                    "VOLUME" => self.audio.set_volume(channel, value, fade),
                    "PAN" => self.audio.set_pan(channel, value, fade),
                    _ => self.audio.set_rate(channel, value, fade),
                };
                result.map_err(|error| self.err(ErrorCode::InvalidArgument).with_detail(error))?;
            }
            _ => return Err(self.err(ErrorCode::Syntax)),
        }
        Ok(())
    }

    pub(super) fn audio_function(&mut self, name: &str, args: &[Value]) -> BasicResult<Value> {
        match name {
            "AUDIOAVAILABLE" if args.is_empty() => Ok(Value::number(if self.audio.available() {
                -1.0
            } else {
                0.0
            })),
            "AUDIOERROR$" if args.is_empty() => {
                self.audio.tick();
                Ok(Value::string(self.audio.error()))
            }
            "SQ" if args.len() == 1 => {
                let channel = self.sound_channel(args[0].as_number()?)?;
                Ok(Value::number(self.audio.sq(channel) as f64))
            }
            "AUDIOSTATE" | "AUDIOPOS" if args.len() == 1 => {
                let channel = self.audio_integer_value(args[0].as_number()?, 1, 32)? as u8;
                Ok(Value::number(if name == "AUDIOSTATE" {
                    self.audio.state(channel) as f64
                } else {
                    self.audio.position(channel)
                }))
            }
            _ => Err(self.err(ErrorCode::ArgumentMismatch)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silent() -> Interpreter {
        let mut interpreter = Interpreter::new();
        interpreter.audio.set_enabled(false);
        interpreter
    }

    fn run(source: &str) -> Interpreter {
        let mut interpreter = silent();
        interpreter.program.load_text(source).unwrap();
        interpreter.run_loaded().unwrap();
        interpreter
    }

    #[test]
    fn audio_cpc_hold_release_flush_and_defaults() {
        let mut i = silent();
        i.process_immediate("SOUND 65,142,1000").unwrap();
        assert_eq!(i.audio.sq(1), 67);
        i.process_immediate("RELEASE 1").unwrap();
        assert_eq!(i.audio.sq(1), 132);
        i.process_immediate("SOUND 129,0,1,0").unwrap();
        i.process_immediate("AUDIO OFF").unwrap();
        assert_eq!(i.audio.sq(1), 4);
        i.process_immediate("SOUND 1,142,,8,,1").unwrap();
        assert_ne!(i.audio.sq(1) & 128, 0);
    }

    #[test]
    fn audio_cpc_manual_duet_completes_both_scores_with_fractional_periods() {
        let source = include_str!("../../samples/s-cpc-duet.bas")
            .replace("Spd=12", "Spd=1")
            .replace("320 SOUND", "320 NOTESA=NOTESA+1:SOUND")
            .replace("500 SOUND", "500 NOTESB=NOTESB+1:SOUND");
        let i = run(&source);
        assert_eq!(i.numeric_variables.get("CH1"), Some(&0.0));
        assert_eq!(i.numeric_variables.get("CH2"), Some(&0.0));
        assert_eq!(i.numeric_variables.get("NOTESA"), Some(&91.0));
        assert_eq!(i.numeric_variables.get("NOTESB"), Some(&44.0));
        assert!(i.gosub_stack.is_empty());
        assert!(i.sound_isr_markers.is_empty());
    }

    #[test]
    fn audio_classic_integer_conversion_rounds_real_expressions_before_range_checks() {
        let i = silent();
        for (input, expected) in [
            (1607.0 / 8.0, 201),
            (2.49, 2),
            (2.5, 3),
            (-2.49, -2),
            (-2.5, -3),
            (4095.49, 4095),
            (-32768.49, -32768),
        ] {
            assert_eq!(i.cpc_integer_value(input, -32768, 4095).unwrap(), expected);
        }
        for input in [4095.5, -32768.5, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert_eq!(
                i.cpc_integer_value(input, -32768, 4095).unwrap_err().code,
                ErrorCode::InvalidArgument
            );
        }
    }

    #[test]
    fn audio_classic_commands_accept_fractional_expressions_from_cpc_scores() {
        let mut i = silent();
        i.process_immediate("ENV 1.2,2.2,5.1,2.1:ENT -1.2,1.1,1.2,1.1")
            .unwrap();
        i.process_immediate("SOUND 65.1,&H647/8,12*1.2,0.1,1.2,1.1,0.2")
            .unwrap();
        i.process_immediate("HELD=SQ(1.2)").unwrap();
        assert_eq!(i.numeric_variables.get("HELD"), Some(&67.0));
        i.process_immediate("RELEASE 1.1").unwrap();
        assert_eq!(i.audio.sq(1), 132);
        let i = run("10 COUNT=0:ON SQ(1.2) GOSUB 100\n20 END\n100 COUNT=COUNT+1:RETURN");
        assert_eq!(i.numeric_variables.get("COUNT"), Some(&1.0));
    }

    #[test]
    fn audio_validates_arguments_without_partial_envelope_changes() {
        for command in [
            "SOUND 0,142",
            "SOUND 8,142",
            "SOUND 1,4096",
            "SOUND 1,4095.5",
            "SOUND 1,-0.5",
            "SOUND 1,142,32768",
            "SOUND 1,142,20,16",
            "RELEASE 8",
            "ENV 0,1,1,1",
            "ENV 1,128,1,1",
            "ENT -16,1,1,1",
            "ENT 1,240,1,1",
            "AUDIO PLAY 33,1",
            "AUDIO PLAY 1.5,1",
            "AUDIO PLAY 1,1,0,2",
            "AUDIO RATE 1,0",
            "AUDIO VOLUME 1,0.5,-1",
        ] {
            let error = silent().process_immediate(command).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument, "{command}: {error}");
        }
        let mut i = silent();
        i.process_immediate("ENV 1,0,15,1,15,-1,2").unwrap();
        i.process_immediate("ENT -2,=142,1,3,1,1").unwrap();
        i.process_immediate("ENV 1").unwrap();
        i.process_immediate("ENT -2").unwrap();
        assert_eq!(
            i.process_immediate("ENV 1,=9,500").unwrap_err().code,
            ErrorCode::Unsupported
        );
    }

    #[test]
    fn audio_on_sq_is_one_shot_and_rearms_after_return() {
        let i = run("10 COUNT=0\n20 ON SQ(1) GOSUB 100\n30 PAUSE 30\n40 RESULT=COUNT\n50 END\n100 COUNT=COUNT+1\n110 IF COUNT<3 THEN ON SQ(1) GOSUB 100\n120 RETURN");
        assert_eq!(i.numeric_variables.get("RESULT"), Some(&3.0));
        assert!(i.sound_isr_markers.is_empty());
    }

    #[test]
    fn audio_global_sq_handler_can_rearm_while_a_named_routine_is_suspended() {
        for (call, routine) in [
            (
                "X=FNWORK()",
                "100 DEF FNWORK()\n110 RELEASE 1\n120 FNWORK=1\n130 FNEND",
            ),
            (
                "CALL WORK",
                "100 DEF SUB WORK()\n110 RELEASE 1\n120 X=1\n130 SUBEND",
            ),
        ] {
            let i = run(&format!(
                "10 COUNT=0:SOUND 65,142,1000\n\
                 20 SOUND 1,142,1000\n\
                 30 SOUND 1,142,1000\n\
                 40 SOUND 1,142,1000\n\
                 50 ON SQ(1) GOSUB 200\n\
                 60 {call}\n\
                 70 END\n\
                 {routine}\n\
                 200 COUNT=COUNT+1\n\
                 210 IF COUNT<2 THEN ON SQ(1) GOSUB 200\n\
                 220 RETURN"
            ));
            assert_eq!(i.numeric_variables.get("COUNT"), Some(&2.0), "{call}");
            assert_eq!(i.numeric_variables.get("X"), Some(&1.0), "{call}");
            assert!(i.sound_isr_markers.is_empty());
        }
    }

    #[test]
    fn audio_on_sq_remains_forbidden_in_named_routine_bodies() {
        for (call, routine, expected) in [
            (
                "X=FNWORK()",
                "100 DEF FNWORK()\n110 ON SQ(1) GOSUB 200\n120 FNWORK=1\n130 FNEND",
                ErrorCode::FunctionForbidden,
            ),
            (
                "CALL WORK",
                "100 DEF SUB WORK()\n110 ON SQ(1) GOSUB 200\n120 SUBEND",
                ErrorCode::SubroutineForbidden,
            ),
        ] {
            let mut i = silent();
            i.program
                .load_text(&format!("10 {call}\n20 END\n{routine}\n200 RETURN"))
                .unwrap();
            assert_eq!(i.run_loaded().unwrap_err().code, expected, "{call}");
        }
    }

    #[test]
    fn audio_di_defers_sq_and_zero_disables_it() {
        let i = run("10 COUNT=0:DI\n20 ON SQ(1) GOSUB 100\n30 BEFORE=COUNT\n40 ON SQ(1) GOSUB 0\n50 EI\n60 AFTERVALUE=COUNT\n70 END\n100 COUNT=COUNT+1:RETURN");
        assert_eq!(i.numeric_variables.get("BEFORE"), Some(&0.0));
        assert_eq!(i.numeric_variables.get("AFTERVALUE"), Some(&0.0));
        let i = run("10 COUNT=0:DI\n20 ON SQ(2) GOSUB 100\n30 BEFORE=COUNT\n40 EI\n50 AFTERVALUE=COUNT\n60 END\n100 COUNT=COUNT+1:RETURN");
        assert_eq!(i.numeric_variables.get("BEFORE"), Some(&0.0));
        assert_eq!(i.numeric_variables.get("AFTERVALUE"), Some(&1.0));
    }

    #[test]
    fn audio_full_queue_wait_services_timer_and_resumes_note_once() {
        let i = run("10 SOUND 65,142,1\n20 SOUND 1,142,1\n30 SOUND 1,142,1\n40 SOUND 1,142,1\n50 AFTER 1 GOSUB 100\n60 SOUND 1,142,1\n70 FINISHED=1\n80 END\n100 FIRED=1:RELEASE 1:RETURN");
        assert_eq!(i.numeric_variables.get("FIRED"), Some(&1.0));
        assert_eq!(i.numeric_variables.get("FINISHED"), Some(&1.0));
        assert!(i.pending_sounds.is_empty());
    }

    #[test]
    fn audio_on_sq_parses_nested_expressions_and_keyword_text_in_strings() {
        let i = run("10 COUNT=0:CHGOSUB=1\n\
             20 ON SQ(CHGOSUB) GOSUB 100\n\
             30 ON SQ((INSTR(\"GOSUB)\",\"G\"))) GOSUB 100\n\
             40 RESULT=COUNT:END\n\
             100 COUNT=COUNT+1:RETURN");
        assert_eq!(i.numeric_variables.get("RESULT"), Some(&2.0));
        for command in ["ON SQ(1 GOSUB 100", "ON SQ(1) GOSUB100"] {
            let mut i = silent();
            i.current_line = Some(10);
            assert_eq!(
                i.execute_on_sq(command).unwrap_err().code,
                ErrorCode::Syntax
            );
        }
    }

    #[test]
    fn audio_full_queue_retry_survives_merge_without_reevaluating_arguments() {
        for merged in ["5 REM INSERTED\n", "5 REM INSERTED\n60\n"] {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("merge.bas"), merged).unwrap();
            let mut i = silent();
            i.root_dir = dir.path().to_path_buf();
            i.current_dir = dir.path().to_path_buf();
            i.program
                .load_text(
                    "10 SOUND 65,142,1\n\
                     20 SOUND 1,142,1\n\
                     30 SOUND 1,142,1\n\
                     40 SOUND 1,142,1\n\
                     50 P=142:AFTER 1 GOSUB 100\n\
                     60 BEFORE=1:SOUND 1,P,1:RESUMED=1\n\
                     70 FINISHED=1:END\n\
                     100 MERGE \"merge.bas\":P=4096:RELEASE 1:RETURN",
                )
                .unwrap();
            i.run_loaded().unwrap();
            assert_eq!(i.numeric_variables.get("P"), Some(&4096.0));
            assert_eq!(i.numeric_variables.get("RESUMED"), Some(&1.0));
            assert_eq!(i.numeric_variables.get("FINISHED"), Some(&1.0));
        }
    }

    #[test]
    fn audio_sq_functions_and_reset_are_available_without_a_device() {
        let mut i = silent();
        i.process_immediate(
            "READY=AUDIOAVAILABLE:STATE=AUDIOSTATE(1):POS=AUDIOPOS(1):MSG$=AUDIOERROR$",
        )
        .unwrap();
        assert_eq!(i.numeric_variables.get("READY"), Some(&0.0));
        assert_eq!(i.numeric_variables.get("STATE"), Some(&0.0));
        i.process_immediate("SOUND 65,142,1000").unwrap();
        i.process_immediate("CLEAR").unwrap();
        assert_eq!(i.audio.sq(1), 4);
        assert_eq!(
            i.process_immediate("A=SQ(3)").unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn audio_modern_commands_load_control_and_unload_real_wav_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sound.wav");
        fs::write(
            &path,
            include_bytes!("../../samples/assets/audio/chime.wav"),
        )
        .unwrap();
        let mut i = silent();
        i.root_dir = dir.path().to_path_buf();
        i.current_dir = dir.path().to_path_buf();
        i.process_immediate("AUDIO LOAD 64,\"sound.wav\"").unwrap();
        i.process_immediate("AUDIO PLAY 32,64,-1,0.5,-1,2").unwrap();
        assert_eq!(i.audio.state(32), 1);
        i.process_immediate("AUDIO PAN 32,1,100:AUDIO VOLUME 32,0.2,100:AUDIO RATE 32,0.5,100")
            .unwrap();
        i.process_immediate("AUDIO PAUSE").unwrap();
        assert_eq!(i.audio.state(32), 2);
        let position = i.audio.position(32);
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(i.audio.position(32), position);
        i.process_immediate("AUDIO RESUME").unwrap();
        assert_eq!(i.audio.state(32), 1);
        i.process_immediate("AUDIO STOP").unwrap();
        assert_eq!(i.audio.state(32), 0);
        i.process_immediate("AUDIO PLAY 1,64,,0.25").unwrap();
        i.process_immediate("AUDIO UNLOAD 64").unwrap();
        assert_eq!(i.audio.state(1), 0);
        assert_eq!(
            i.process_immediate("AUDIO PLAY 1,64").unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn audio_loading_reports_file_errors_and_clear_unloads_assets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-sound.wav");
        fs::write(&path, b"not an audio stream").unwrap();
        let mut i = silent();
        i.root_dir = dir.path().to_path_buf();
        i.current_dir = dir.path().to_path_buf();
        assert_eq!(
            i.process_immediate("AUDIO LOAD 1,\"not-a-sound.wav\"")
                .unwrap_err()
                .code,
            ErrorCode::InvalidFileData
        );
        assert_eq!(
            i.process_immediate("AUDIO LOAD 1,\"missing.wav\"")
                .unwrap_err()
                .code,
            ErrorCode::FileNotFound
        );
        fs::write(
            &path,
            include_bytes!("../../samples/assets/audio/chime.wav"),
        )
        .unwrap();
        i.process_immediate("AUDIO LOAD 1,\"not-a-sound.wav\"")
            .unwrap();
        i.process_immediate("CLEAR").unwrap();
        assert_eq!(
            i.process_immediate("AUDIO PLAY 1,1").unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn audio_end_and_stop_silence_classic_queues() {
        for ending in ["END", "STOP"] {
            let mut i = run(&format!("10 SOUND 1,142,1000\n20 {ending}"));
            assert_eq!(i.audio.sq(1), 4);
        }
    }

    #[test]
    fn audio_sq_return_restores_interrupts_and_renum_updates_target() {
        let mut i = silent();
        i.program.load_text("10 COUNT=0:ON SQ(1) GOSUB 100\n20 ON SQ(2) GOSUB 100\n30 END\n100 COUNT=COUNT+1:DI:RETURN").unwrap();
        i.execute_renum("1000,10").unwrap();
        i.run_loaded().unwrap();
        assert_eq!(i.numeric_variables.get("COUNT"), Some(&2.0));
        assert!(i.interrupts_enabled);
    }

    #[test]
    fn audio_sq_returns_to_the_same_return_statement_in_normal_and_debug_runs() {
        for debug in [false, true] {
            let mut i = silent();
            if debug {
                i.set_debugger(Debugger::scripted([]));
            }
            i.program
                .load_text(
                    "10 COUNT=0:GOSUB 100\n\
                     20 RESULT=COUNT\n\
                     30 END\n\
                     100 COUNT=COUNT+1\n\
                     110 IF COUNT<2 THEN ON SQ(1) GOSUB 100\n\
                     120 RETURN\n\
                     130 ERROR 200",
                )
                .unwrap();
            i.run_loaded().unwrap_or_else(|error| {
                panic!("debug={debug}: {error}");
            });
            assert_eq!(
                i.numeric_variables.get("RESULT"),
                Some(&2.0),
                "debug={debug}"
            );
            assert!(i.gosub_stack.is_empty(), "debug={debug}");
            assert!(i.sound_isr_markers.is_empty(), "debug={debug}");
        }
    }
}
