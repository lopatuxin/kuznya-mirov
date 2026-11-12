use std::time::Duration;

use crate::{GlobalState, Lua, LuaApi, LuaSandboxApi, LuaValue, SafeOption, SandboxConfig, Stdlib};

#[test]
fn test_execute_sandboxed_isolates_globals() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let results = vm
        .main_state()
        .execute_sandboxed(
            "sandbox_value = 42; return sandbox_value, _G == _ENV",
            &SandboxConfig::default(),
        )
        .unwrap();

    assert_eq!(results[0].as_integer(), Some(42));
    assert!(results[1].bvalue());
    assert!(vm.get_global("sandbox_value").unwrap().is_none());
}

#[test]
fn test_sandbox_blocks_dangerous_basic_functions_by_default() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let results = vm
        .main_state()
        .execute_sandboxed(
            "return require, load, loadfile, dofile, collectgarbage",
            &SandboxConfig::default(),
        )
        .unwrap();

    assert!(results.iter().all(|value| value.is_nil()));
}

#[test]
fn test_load_sandboxed_uses_own_env() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();
    vm.set_global("shared_value", crate::LuaValue::integer(7))
        .unwrap();

    let config = SandboxConfig::default();
    let func = vm
        .main_state()
        .load_sandboxed(
            "local local_only = 11; return shared_value, local_only",
            &config,
        )
        .unwrap();
    let results: Vec<crate::LuaValue> = vm.main_state().call(func, vec![]).unwrap();

    assert!(results[0].is_nil());
    assert_eq!(results[1].as_integer(), Some(11));
}

#[test]
fn test_sandbox_can_enable_package_require_explicitly() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default()
        .with_stdlib(Stdlib::Package)
        .allow_require();
    let results = vm
        .main_state()
        .execute_sandboxed(
            "local p = require('math'); return type(p), type(require)",
            &config,
        )
        .unwrap();

    assert_eq!(results[0].as_str(), Some("table"));
    assert_eq!(results[1].as_str(), Some("function"));
}

#[test]
fn test_sandbox_can_inject_custom_globals() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default().with_global("answer", LuaValue::integer(99));
    let results = vm
        .main_state()
        .execute_sandboxed("return answer, _G.answer == answer", &config)
        .unwrap();

    assert_eq!(results[0].as_integer(), Some(99));
    assert!(results[1].bvalue());
    assert!(vm.get_global("answer").unwrap().is_none());
}

#[test]
fn test_sandbox_instruction_limit_stops_infinite_loops() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default().with_instruction_limit(1_000);
    let err = vm
        .main_state()
        .execute_sandboxed("while true do end", &config)
        .unwrap_err();
    let full = vm.main_state().get_full_error(err);

    assert!(full.message.contains("sandbox instruction limit exceeded"));
}

#[test]
fn test_sandbox_memory_limit_blocks_runtime_allocations() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default().with_memory_limit(0);
    let err = vm
        .main_state()
        .execute_sandboxed("local t = {}; return t", &config)
        .unwrap_err();
    let full = vm.main_state().get_full_error(err);

    assert!(matches!(full.kind, crate::lua_vm::LuaError::OutOfMemory));
}

// ===== Persistent instruction budget (LuaState::set_instruction_budget) =====

#[test]
fn test_instruction_budget_is_shared_across_calls() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default();
    let chunk = vm
        .main_state()
        .load_sandboxed("for i = 1, 10 do end", &config)
        .unwrap();

    vm.main_state().set_instruction_budget(50);
    // Each call spends its own instructions against the same shared budget.
    vm.main_state().call(chunk, vec![]).unwrap();
    let after_one = vm.main_state().instruction_budget_remaining().unwrap();
    vm.main_state().call(chunk, vec![]).unwrap();
    let after_two = vm.main_state().instruction_budget_remaining().unwrap();

    assert!(after_one < 50);
    assert!(after_two < after_one);
}

#[test]
fn test_instruction_budget_exhaustion_repeats_on_every_later_operation() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default();
    let chunk = vm
        .main_state()
        .load_sandboxed("while true do end", &config)
        .unwrap();

    vm.main_state().set_instruction_budget(10);
    let err = vm.main_state().call(chunk, vec![]).unwrap_err();
    assert!(matches!(
        err,
        crate::lua_vm::LuaError::InstructionBudgetExceeded
    ));
    assert_eq!(vm.main_state().instruction_budget_remaining(), Some(0));

    // Exhausted budget keeps erroring on every later call, until reset.
    let err2 = vm.main_state().call(chunk, vec![]).unwrap_err();
    assert!(matches!(
        err2,
        crate::lua_vm::LuaError::InstructionBudgetExceeded
    ));
}

#[test]
fn test_instruction_budget_survives_pcall() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default();
    let chunk = vm
        .main_state()
        .load_sandboxed("while true do pcall(function() end) end", &config)
        .unwrap();

    vm.main_state().set_instruction_budget(200);
    let err = vm.main_state().call(chunk, vec![]).unwrap_err();
    assert!(matches!(
        err,
        crate::lua_vm::LuaError::InstructionBudgetExceeded
    ));
}

#[test]
fn test_instruction_budget_reset_allows_more_code() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default();
    let chunk = vm
        .main_state()
        .load_sandboxed("for i = 1, 1000 do end", &config)
        .unwrap();

    vm.main_state().set_instruction_budget(10);
    let err = vm.main_state().call(chunk, vec![]).unwrap_err();
    assert!(matches!(
        err,
        crate::lua_vm::LuaError::InstructionBudgetExceeded
    ));

    // Setting the budget again lifts the exhaustion: the same, previously
    // blocked code now runs to completion.
    vm.main_state().set_instruction_budget(1_000_000);
    vm.main_state().call(chunk, vec![]).unwrap();
    assert!(vm.main_state().instruction_budget_remaining().unwrap() < 1_000_000);
}

#[test]
fn test_no_instruction_budget_means_no_count() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    assert_eq!(vm.main_state().instruction_budget_remaining(), None);

    let config = SandboxConfig::default();
    let chunk = vm
        .main_state()
        .load_sandboxed("for i = 1, 1000 do end", &config)
        .unwrap();
    vm.main_state().call(chunk, vec![]).unwrap();
    // Never set: still uncounted after running real code.
    assert_eq!(vm.main_state().instruction_budget_remaining(), None);
}

#[test]
fn test_sandbox_config_instruction_limit_unaffected_by_budget() {
    // `SandboxConfig::instruction_limit` (per-call) keeps working exactly as
    // before, independent of the new persistent budget.
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default().with_instruction_limit(1_000);
    let err = vm
        .main_state()
        .execute_sandboxed("while true do end", &config)
        .unwrap_err();
    let full = vm.main_state().get_full_error(err);
    assert!(full.message.contains("sandbox instruction limit exceeded"));
}

#[test]
fn test_instruction_budget_exhaustion_reports_frames_structurally() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default();
    let chunk = vm
        .main_state()
        .load_sandboxed("local function f()\nwhile true do end\nend\nf()", &config)
        .unwrap();

    vm.main_state().set_instruction_budget(10);
    let err = vm.main_state().call(chunk, vec![]).unwrap_err();
    let full = vm.main_state().get_full_error(err);

    assert!(!full.frames.is_empty(), "{full:?}");
    assert_eq!(full.frames[0].line, 2, "{full:?}");
}

#[test]
fn test_lua_public_budget_methods_delegate_to_lua_state() {
    let mut lua = Lua::new(SafeOption::default());
    lua.open_stdlibs(&[Stdlib::All]).unwrap();

    assert_eq!(lua.instruction_budget_remaining(), None);

    lua.set_instruction_budget(50);
    lua.execute_sandboxed("for i = 1, 10 do end", &SandboxConfig::default())
        .unwrap();
    let remaining = lua.instruction_budget_remaining().unwrap();
    assert!(remaining < 50, "{remaining}");

    lua.set_instruction_budget(1_000_000);
    assert_eq!(lua.instruction_budget_remaining(), Some(1_000_000));
}

#[test]
fn test_sandbox_timeout_stops_infinite_loops() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(Stdlib::All).unwrap();

    let config = SandboxConfig::default().with_timeout(Duration::ZERO);
    let err = vm
        .main_state()
        .execute_sandboxed("while true do end", &config)
        .unwrap_err();
    let full = vm.main_state().get_full_error(err);

    assert!(full.message.contains("sandbox timeout exceeded"));
}
