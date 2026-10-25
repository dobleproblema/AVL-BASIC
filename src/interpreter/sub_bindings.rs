use super::*;

// Definitions own the plan and spare storage; active calls own all saved values.
// Slot caches refer to the existing visible variable table, preserving dynamic
// visibility in nested routines, and are invalidated by NumericVariables::clear.
#[derive(Debug)]
pub(super) struct SubBindingPlan {
    names: Vec<BindingName>,
    params: Vec<usize>,
    locals: Vec<LocalBinding>,
    spare: RefCell<BindingStorage>,
}

#[derive(Debug)]
struct BindingName {
    name: String,
    numeric: CachedNumericSlot,
}

#[derive(Debug)]
struct LocalBinding {
    name: usize,
    dimensions: Option<Vec<LocalDimension>>,
}

#[derive(Debug)]
struct LocalDimension {
    source: String,
    compiled: RefCell<Option<Rc<CompiledNumberExpr>>>,
}

#[derive(Debug, Default)]
struct BindingStorage {
    saved: Vec<u8>,
    numeric: Vec<(usize, Option<f64>)>,
    strings: Vec<(usize, Option<String>)>,
    arrays: Vec<(usize, SavedArrayBinding)>,
    aliases: Vec<(usize, Option<String>)>,
    dimensions: Vec<usize>,
}

impl SubBindingPlan {
    pub(super) fn new(params: &[String], locals: &[LocalSpec]) -> Self {
        let mut names = Vec::<BindingName>::new();
        let mut intern = |name: &str| {
            if let Some(index) = names.iter().position(|entry| entry.name == name) {
                return index;
            }
            let index = names.len();
            names.push(BindingName {
                name: name.to_owned(),
                numeric: CachedNumericSlot::default(),
            });
            index
        };
        let params = params.iter().map(|name| intern(name)).collect();
        let locals = locals
            .iter()
            .map(|spec| LocalBinding {
                name: intern(local_spec_name(spec)),
                dimensions: match spec {
                    LocalSpec::Scalar(_) => None,
                    LocalSpec::Array { dims, .. } => Some(
                        dims.iter()
                            .map(|source| LocalDimension {
                                source: source.clone(),
                                compiled: RefCell::new(None),
                            })
                            .collect(),
                    ),
                },
            })
            .collect();
        Self {
            names,
            params,
            locals,
            spare: RefCell::default(),
        }
    }
}

pub(super) struct SubCallBindings<'a> {
    plan: &'a SubBindingPlan,
    routine_name: &'a str,
    routine_value: SavedSubName,
    storage: BindingStorage,
}

impl<'a> SubCallBindings<'a> {
    pub(super) fn new(interpreter: &Interpreter, sub: &'a UserSub) -> Self {
        let routine_name = sub.name.as_ref();
        let routine_value = if routine_name.ends_with('$') {
            SavedSubName::String(interpreter.string_variables.get(routine_name).cloned())
        } else {
            SavedSubName::Numeric(interpreter.numeric_variables.get(routine_name).copied())
        };
        let plan = &sub.binding_plan;
        let mut storage = std::mem::take(&mut *plan.spare.borrow_mut());
        storage.saved.resize(plan.names.len(), 0);
        Self {
            plan,
            routine_name,
            routine_value,
            storage,
        }
    }

    fn save_numeric(&mut self, interpreter: &mut Interpreter, index: usize) -> usize {
        let binding = &self.plan.names[index];
        let slot = interpreter
            .numeric_variables
            .resolve_cached_slot(&binding.name, &binding.numeric);
        if self.storage.saved[index] & 1 == 0 {
            let value = &interpreter.numeric_variables.slots[slot];
            self.storage
                .numeric
                .push((index, value.present.then_some(value.value)));
            self.storage.saved[index] |= 1;
        }
        slot
    }

    fn save_string(&mut self, interpreter: &mut Interpreter, index: usize) {
        if self.storage.saved[index] & 2 == 0 {
            let name = &self.plan.names[index].name;
            // Keep the map key when present, moving only the hidden value.
            let value = interpreter
                .string_variables
                .get_mut(name)
                .map(std::mem::take);
            self.storage.strings.push((index, value));
            self.storage.saved[index] |= 2;
        }
    }

    fn save_alias(&mut self, interpreter: &mut Interpreter, index: usize) {
        if self.storage.saved[index] & 4 == 0 {
            let value = interpreter
                .array_aliases
                .remove(&self.plan.names[index].name);
            if value.is_some() {
                interpreter.arrays.invalidate_cached_slots();
            }
            self.storage.aliases.push((index, value));
            self.storage.saved[index] |= 4;
        }
    }

    pub(super) fn bind_arguments(
        &mut self,
        interpreter: &mut Interpreter,
        args: &mut dyn ExactSizeIterator<Item = Value>,
    ) -> BasicResult<()> {
        for (position, value) in args.enumerate() {
            let index = self.plan.params[position];
            let name = &self.plan.names[index].name;
            match value {
                Value::Number(value) => {
                    if name.ends_with('$') {
                        return Err(interpreter.err(ErrorCode::TypeMismatch));
                    }
                    let slot = self.save_numeric(interpreter, index);
                    interpreter.numeric_variables.insert_resolved(slot, value);
                }
                Value::Str(value) => {
                    if !name.ends_with('$') {
                        return Err(interpreter.err(ErrorCode::TypeMismatch));
                    }
                    self.save_string(interpreter, index);
                    set_string_binding_ref(&mut interpreter.string_variables, name, value);
                }
                Value::ArrayRef(source) => {
                    let Some(array) = interpreter.arrays.get(&source) else {
                        return Err(interpreter.err(ErrorCode::Undefined));
                    };
                    if name.ends_with('$') != array.is_string() {
                        return Err(interpreter.err(ErrorCode::TypeMismatch));
                    }
                    let slot = self.save_numeric(interpreter, index);
                    self.save_string(interpreter, index);
                    self.save_alias(interpreter, index);
                    interpreter.numeric_variables.slots[slot].value = 0.0;
                    interpreter.numeric_variables.slots[slot].present = false;
                    interpreter.string_variables.remove(name);
                    interpreter.set_array_alias_binding(name, source);
                }
            }
        }
        Ok(())
    }

    pub(super) fn bind_locals(&mut self, interpreter: &mut Interpreter) -> BasicResult<()> {
        for local in &self.plan.locals {
            let index = local.name;
            let name = &self.plan.names[index].name;
            if let Some(dimensions) = &local.dimensions {
                self.storage.dimensions.clear();
                for dimension in dimensions {
                    // Compile on first invocation, so invalid dimensions in an
                    // uncalled definition do not become eager program errors.
                    let compiled = dimension.compiled.borrow().clone();
                    let compiled = if let Some(compiled) = compiled {
                        compiled
                    } else {
                        let compiled = Rc::new(
                            compile_number_expression(&dimension.source)
                                .map_err(|error| interpreter.with_current_line(error))?,
                        );
                        *dimension.compiled.borrow_mut() = Some(compiled.clone());
                        compiled
                    };
                    let value = compiled
                        .eval(interpreter)
                        .map_err(|error| interpreter.with_current_line(error))?;
                    self.storage.dimensions.push(
                        array_dimension_bound(value)
                            .map_err(|error| interpreter.with_current_line(error))?,
                    );
                }
                let array = interpreter
                    .sub_runtime
                    .local_array_pool
                    .take_or_new(name, &self.storage.dimensions)
                    .map_err(|error| interpreter.with_current_line(error))?;
                self.save_alias(interpreter, index);
                let previous = interpreter.take_local_array_binding(name);
                self.storage.arrays.push((index, previous));
                set_array_binding_ref(&mut interpreter.arrays, name, array);
            } else if name.ends_with('$') {
                self.save_string(interpreter, index);
                set_string_binding_ref(&mut interpreter.string_variables, name, String::new());
            } else {
                let slot = self.save_numeric(interpreter, index);
                interpreter.numeric_variables.insert_resolved(slot, 0.0);
            }
        }
        Ok(())
    }

    pub(super) fn restore(mut self, interpreter: &mut Interpreter) {
        for (index, value) in self.storage.numeric.drain(..) {
            let binding = &self.plan.names[index];
            let slot = interpreter
                .numeric_variables
                .resolve_cached_slot(&binding.name, &binding.numeric);
            interpreter.numeric_variables.slots[slot].value = value.unwrap_or(0.0);
            interpreter.numeric_variables.slots[slot].present = value.is_some();
        }
        for (index, value) in self.storage.strings.drain(..) {
            let name = &self.plan.names[index].name;
            if let Some(value) = value {
                set_string_binding_ref(&mut interpreter.string_variables, name, value);
            } else {
                interpreter.string_variables.remove(name);
            }
        }
        for (index, value) in self.storage.aliases.drain(..) {
            let name = &self.plan.names[index].name;
            if let Some(value) = value {
                interpreter.set_array_alias_binding(name, value);
            } else {
                interpreter.remove_array_alias_binding(name);
            }
        }
        for (index, previous) in self.storage.arrays.drain(..) {
            let name = &self.plan.names[index].name;
            if let Some(array) = interpreter.arrays.remove(name) {
                interpreter.sub_runtime.local_array_pool.recycle(array);
            }
            interpreter.restore_local_array_binding(name, previous);
        }
        match self.routine_value {
            SavedSubName::Numeric(Some(value)) => {
                set_numeric_binding_ref(
                    &mut interpreter.numeric_variables,
                    self.routine_name,
                    value,
                );
            }
            SavedSubName::Numeric(None) => {
                interpreter.numeric_variables.remove(self.routine_name);
            }
            SavedSubName::String(Some(value)) => {
                set_string_binding_ref(&mut interpreter.string_variables, self.routine_name, value);
            }
            SavedSubName::String(None) => {
                interpreter.string_variables.remove(self.routine_name);
            }
        }
        self.storage.saved.fill(0);
        self.storage.dimensions.clear();
        *self.plan.spare.borrow_mut() = self.storage;
    }
}
