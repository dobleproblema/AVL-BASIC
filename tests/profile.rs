use avl_basic::{ErrorCode, Interpreter};

fn program(source: &str) -> Interpreter {
    let mut i = Interpreter::new();
    for line in source.lines() {
        i.process_immediate(line).unwrap();
    }
    i
}

fn section<'a>(report: &'a str, name: &str) -> impl Iterator<Item = &'a str> {
    report
        .split("\n\n")
        .find(|block| block.lines().next() == Some(name))
        .into_iter()
        .flat_map(|block| block.lines().skip(2))
}

fn visits(report: &str, module: &str, line: i32) -> u64 {
    let has_modules = report.contains("\nMODULES\n");
    let module_id = section(report, "MODULES").find_map(|row| {
        let (id, path) = row.trim_start().split_once(char::is_whitespace)?;
        (path.trim_start() == module || path.trim_start().ends_with(module)).then_some(id)
    });
    section(report, "LINES")
        .find_map(|row| {
            let mut cells = row.split_whitespace();
            if has_modules && cells.next() != module_id {
                return None;
            }
            if cells.next()?.parse::<i32>().ok()? == line {
                cells.next()?.parse().ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("missing {module} {line} in {report}"))
}

fn routine_counts(report: &str, name: &str) -> (u64, u64) {
    let has_modules = report.contains("\nMODULES\n");
    let row = section(report, "ROUTINES")
        .find(|row| row.ends_with(name))
        .unwrap();
    let mut cells = row.split_whitespace().skip(usize::from(has_modules));
    (
        cells.next().unwrap().parse().unwrap(),
        cells.next().unwrap().parse().unwrap(),
    )
}

#[test]
fn visits_and_calls_include_nested_runs_and_inline_statements() {
    let mut i = program("10 FOR I=1 TO 4\n20 CALL A : Z=Z+1\n30 NEXT I\n40 PRINT X,Z\n50 END\n100 DEF SUB A\n110 IF 1 THEN CALL B\n120 SUBEND\n200 DEF SUB B\n210 X=X+1\n220 SUBEND");
    i.process_immediate("profile").unwrap();
    let p = i.take_output();
    for line in [20, 30, 110, 120, 210, 220] {
        assert_eq!(visits(&p, "main", line), 4);
    }
    assert_eq!(routine_counts(&p, "SUB A"), (4, 8));
    assert_eq!(routine_counts(&p, "SUB B"), (4, 8));
    i.process_immediate("RUN").unwrap();
    assert_eq!(i.take_output(), p.split("PROFILE  elapsed").next().unwrap());
}

#[test]
fn jumps_and_loops_within_a_line_count_each_reentry() {
    let mut i = program("10 X=X+1 : IF X<4 THEN 10\n20 FOR I=1 TO 3 : Y=Y+1 : NEXT I\n30 END");
    i.process_immediate("PROFILE").unwrap();
    let p = i.take_output();
    assert_eq!(visits(&p, "main", 10), 4);
    assert_eq!(visits(&p, "main", 20), 3);
}

#[test]
fn the_routine_column_uses_the_declared_function_name() {
    let mut i =
        program("10 PRINT FNDoble(2)\n20 END\n100 DEF FNDoble(X)\n110 FNDoble=X*2\n120 FNEND");
    i.process_immediate("PROFILE").unwrap();
    let p = i.take_output();
    assert_eq!(routine_counts(&p, "FNDOBLE"), (1, 2));
    assert!(!p.contains("FN FNDOBLE"));
}

#[test]
fn stop_error_and_ordinary_identifiers() {
    let mut i = program("10 profile=4\n20 STOP\n30 PRINT profile\n40 END");
    i.process_immediate("PROFILE").unwrap();
    let p = i.take_output();
    assert_eq!(visits(&p, "main", 20), 1);
    assert!(!section(&p, "LINES").any(|row| row.split_whitespace().next() == Some("30")));
    i.process_immediate("CONT").unwrap();
    assert_eq!(i.take_output().trim(), "4");
    i.process_immediate("20 ERROR 26").unwrap();
    assert!(i.process_immediate("PROFILE").is_err());
    assert_eq!(visits(&i.take_output(), "main", 20), 1);
    i.process_immediate("NEW").unwrap();
    i.process_immediate("Profile=9").unwrap();
    i.process_immediate("PRINT Profile").unwrap();
    assert_eq!(i.take_output().trim(), "9");
    i.process_immediate("10 PROFILE").unwrap();
    assert_eq!(
        i.process_immediate("RUN").unwrap_err().code,
        ErrorCode::ImmediateCommand
    );
}

#[test]
fn import_and_chain_keep_separate_sources_and_nested_calls() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("lib.bas"),
        "10 DEF SUB S\n20 PRINT 23\n30 SUBEND\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("next.bas"),
        "10 CALL B\n20 END\n30 DEF SUB B\n40 PRINT 12\n50 SUBEND\n",
    )
    .unwrap();
    let mut i = program("10 IMPORT \"lib.bas\" AS ModA\n20 CALL ModA.S\n30 CHAIN \"next.bas\"");
    i.current_dir = dir.path().to_path_buf();
    i.root_dir = dir.path().to_path_buf();
    i.process_immediate("PROFILE").unwrap();
    let p = i.take_output();
    assert_eq!(visits(&p, "lib.bas", 20), 1);
    assert_eq!(routine_counts(&p, "SUB B").0, 1);
    assert_eq!(visits(&p, "main", 20), 1);
}
