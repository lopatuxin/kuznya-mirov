use crate::LuaApi;
use crate::lua_value::LuaValue;
/// Tests for C function calling
use crate::lua_vm::{GlobalState, LuaResult, LuaState, SafeOption};

/// C function with no return value
fn test_no_return(_state: &mut LuaState) -> LuaResult<usize> {
    Ok(0)
}

#[test]
fn test_call_c_function_basic() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(crate::stdlib::Stdlib::All).unwrap();

    // Register a simple C function
    let c_func = LuaValue::cfunction(test_no_return);
    vm.set_global("test_func", c_func).unwrap();

    // Call it from Lua
    let result = vm.main_state().execute(
        r#"
        test_func()
        return 42
        "#,
    );

    // Should not error
    assert!(result.is_ok(), "C function call failed: {:?}", result.err());
}

#[test]
fn test_call_c_function_in_expression() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(crate::stdlib::Stdlib::All).unwrap();

    // Register C function
    let c_func = LuaValue::cfunction(test_no_return);
    vm.set_global("cfunc", c_func).unwrap();

    // Use in expression
    let result = vm.main_state().execute(
        r#"
        local x = cfunc()
        assert(x == nil)
        "#,
    );

    assert!(
        result.is_ok(),
        "C function in expression failed: {:?}",
        result.err()
    );
}

#[test]
fn test_call_c_function_multiple_times() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(crate::stdlib::Stdlib::All).unwrap();

    // Register C function
    let c_func = LuaValue::cfunction(test_no_return);
    vm.set_global("cfunc", c_func).unwrap();

    // Call multiple times
    let result = vm.main_state().execute(
        r#"
        for i = 1, 10 do
            cfunc()
        end
        "#,
    );

    assert!(
        result.is_ok(),
        "Multiple C function calls failed: {:?}",
        result.err()
    );
}

#[test]
fn test_c_function_in_tail_call() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(crate::stdlib::Stdlib::All).unwrap();

    // Register C function
    let c_func = LuaValue::cfunction(test_no_return);
    vm.set_global("cfunc", c_func).unwrap();

    // Use in tail call position
    let result = vm.main_state().execute(
        r#"
        local function wrapper()
            return cfunc()
        end
        wrapper()
        "#,
    );

    assert!(
        result.is_ok(),
        "C function tail call failed: {:?}",
        result.err()
    );
}

/// A host function taking the raw argument list sees the real argument
/// count, trailing `nil`s included — unlike a fixed-arity `Fn(A, B) -> R`,
/// which cannot tell "not passed" from "passed as nil".
#[test]
fn test_create_function_vec_args_sees_trailing_nil() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(crate::stdlib::Stdlib::All).unwrap();

    LuaApi::register_function(vm.main_state(), "count", |args: Vec<LuaValue>| -> i64 {
        args.len() as i64
    })
    .unwrap();

    let result = vm
        .main_state()
        .execute("return count(1, nil), count(nil), count(), count(1, nil, nil)")
        .unwrap();
    assert_eq!(result[0].as_integer(), Some(2));
    assert_eq!(result[1].as_integer(), Some(1));
    assert_eq!(result[2].as_integer(), Some(0));
    assert_eq!(result[3].as_integer(), Some(3));
}

/// An error returned by a host function registered through `create_function`
/// gets the calling Lua line prepended, like `luaL_error` in real Lua — the
/// host function's own C frame carries no line info, so the location must
/// come from its caller.
#[test]
fn test_create_function_error_gets_caller_line() {
    let mut vm = GlobalState::new(SafeOption::default());
    vm.open_stdlib(crate::stdlib::Stdlib::All).unwrap();

    LuaApi::register_function(vm.main_state(), "boom", |_x: i64| -> Result<(), String> {
        Err("boom".to_string())
    })
    .unwrap();

    let err = vm
        .main_state()
        .execute("local function f()\n  boom(1)\nend\nf()")
        .unwrap_err();
    let full = vm.main_state().get_full_error(err);
    assert!(
        full.message.starts_with("chunk:2: boom"),
        "{}",
        full.message
    );
    // «Фаза 07» → требование 12: место ошибки доступно и структурно, не только текстом —
    // deepest frame is `f`'s own call to `boom`, on line 2 of the default `@chunk` source.
    assert_eq!(full.frames[0].source, "@chunk", "{:?}", full.frames);
    assert_eq!(full.frames[0].line, 2, "{:?}", full.frames);
}
