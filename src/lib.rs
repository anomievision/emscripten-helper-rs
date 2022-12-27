// #![feature(offset_to)]

/*!
emscripten-helper is a small library which attempts to provide a convenient way to interface the
Rust and JavaScript worlds.

It currently requires using the rust nightly channel and either the `asmjs-unknown-emscripten`
or `wasm-unknown-emscripten` targets. See the README for any help setting up a development
environment.

# Javascript helpers

`HELPERJS` provides a number of helper JavaScript functions to store and convert JavaScript objects to be
used from Rust. All of these can be accessed through the JavaScript global `HELPERJS` after [`init`] has
been called. 

## `HELPERJS` global

A global JavaScript object named `HELPERJS` is created by [`init`]. It contains all the helper JavaScript
functions as well as an object table to keep the JavaScript objects that are held by Rust code. See
[`JSObject`] for more details.

## `HELPERJS.loadObject(index)`

Loads a JavaScript object from the object table and returns it.

## `HELPERJS.storeObject(js_object)`

Stores an object into the object table and returns the index. The result is commonly wrapped into
a [`JSObject`] by [`js_obj!`].

## Private helper functions

### `HELPERJS.releaseObject(index)`

Removes an object from the object table. For use in the `Drop` implementation of [`JSObject`] only.
Releasing an object that is still refered to by a `JSObject` will cause problems.

### `HELPERJS.copyStringToHeap(js_string)`

Copy a JavaScript string into the Rust heap and returns the address. The string is stored as a 32-bit
unsigned integer containing the number of 16-bit code units (not bytes or characters!) in the buffer,
followed by that number of 16-bit code units from the UTF-16 string.

Used by [`js_string!`] and the implementation of `std::convert::From<JSObject> for String`.

### `HELPERJS.copyStringFromHeap(pointer)`

Convert a String stored on the Rust heap (as described in [`copyStringToHeap`]) into a JavaScript
string. The memory is freed after conversion, so the pointer should be considered invalid by the caller.

[`init`]:     fn.init.html
[`js_obj!`]:  macro.js_obj.html
[`js_int!`]:    macro.js_int.html
[`js_double!`]: macro.js_double.html
[`js_string!`]: macro.js_string.html
[`js!`]:      macro.js.html
[`JSObject`]: struct.JSObject.html
[`copyStringToHeap`]: index.html#rsjscopystringtoheapjs_string
*/ 

use std::ptr;
use std::rc::Rc;

type em_callback_func = unsafe extern "C" fn(context: *mut std::os::raw::c_void);

/// This module declares C functions provided by either emscripten or the C standard library.
/// These are all unsafe, and most are described in [the emscripten documentation](http://kripken.github.io/emscripten-site/docs/api_reference/emscripten.h.html).
/// You should probably avoid using these directly.
pub mod emscripten {
    use std;
    
    // These functions are used in macros, so sometimes they will be considered dead.
    extern "C" {
        /// Run a snippet of JavaScript with no arguments and no return value.
        ///
        /// See [emscripten_run_script (emscripten documentation)](http://kripken.github.io/emscripten-site/docs/api_reference/emscripten.h.html#c.emscripten_run_script) for details.
        #[allow(dead_code)]
        pub fn emscripten_run_script(script: *const std::os::raw::c_char);
        /// Run a snippet of JavaScript with no arguments and an integer return value.
        ///
        /// See [emscripten_run_script (emscripten documentation)](http://kripken.github.io/emscripten-site/docs/api_reference/emscripten.h.html#c.emscripten_run_script_int) for details.
        #[allow(dead_code)]
        pub fn emscripten_run_script_int(script: *const std::os::raw::c_char) -> std::os::raw::c_int;
        /// Run a snippet of JavaScript with no arguments that returns a C string.
        ///
        /// See [emscripten_run_script (emscripten documentation)](http://kripken.github.io/emscripten-site/docs/api_reference/emscripten.h.html#c.emscripten_run_script_string) for details.
        #[allow(dead_code)]
        pub fn emscripten_run_script_string(script: *const std::os::raw::c_char) -> *const std::os::raw::c_char;
        
        /// Run a snippet of JavaScript with no arguments.
        ///
        /// This function is slightly faster than `emscripten_run_script` but requires that
        /// the code snippet has a static lifetime. The emscripten compiler uses some magic
        /// to inline the code directly into the generated asm.js (or webassembly scaffolding code)
        /// and this avoids using `eval()`.
        #[allow(dead_code)]
        pub fn emscripten_asm_const(code: *const std::os::raw::c_char, arg_sigs: *const std::os::raw::c_char);
        /// Run a snippet of JavaScript with any arguments that can be doubles or ints, and returns an int.
        ///
        /// This function is slightly faster than `emscripten_run_script_int` but requires that
        /// the code snippet has a static lifetime. The emscripten compiler uses some magic
        /// to inline the code directly into the generated asm.js (or webassembly scaffolding code)
        /// and this avoids using `eval()`.
        /// Unlike `emscripten_run_script_int`, this function also allows passing arguments to the JavaScript
        /// side.
        #[allow(dead_code)]
        pub fn emscripten_asm_const_int(code: *const std::os::raw::c_char, arg_sigs: *const std::os::raw::c_char, ...) -> std::os::raw::c_int;
        /// Run a snippet of JavaScript with any arguments that can be doubles or ints, and returns a double.
        ///
        /// This is a version of `emscripten_asm_const_int` which doesn't convert the JavaScript result to an
        /// int, but instead preserves it as a double-precision floating point.
        #[allow(dead_code)]
        pub fn emscripten_asm_const_double(code: *const std::os::raw::c_char, arg_sigs: *const std::os::raw::c_char, ...) -> std::os::raw::c_double;
        #[allow(dead_code)]
        pub fn emscripten_pause_main_loop();
        #[allow(dead_code)]
        pub fn emscripten_set_main_loop(func: extern fn(), fps: std::os::raw::c_int, infinite: std::os::raw::c_int);
        #[allow(dead_code)]
        pub fn emscripten_set_main_loop_arg(
            func: crate::em_callback_func,
            arg: *mut std::os::raw::c_void,
            fps: std::os::raw::c_int,
            simulate_infinite_loop: std::os::raw::c_int,
        );

        /// See [free(3)](https://linux.die.net/man/3/free)
        pub fn free(p: *mut u8);
    }
}

fn string_from_js(ptr: *mut u16) -> String {
    unsafe {
        let size : u32 = *(ptr as *const _ as *const u32);
        let string_slice : &'static [u16] = std::slice::from_raw_parts(ptr, size as usize);
        let result = String::from_utf16_lossy(string_slice);
        emscripten::free(ptr as *mut _);
        result
    }
}

/// Run a snippet of JavaScript code.
pub fn js_eval(code: &'static [u8]) {
    let arg_sigs = [0].as_ptr();

    unsafe {
        emscripten::emscripten_asm_const(code as *const _ as *const std::os::raw::c_char, arg_sigs as *const _ as *const std::os::raw::c_char);
    }
}

/// Helper macro used by [`js!`], [`js_int!`], [`js_double!`], [`js_string!`] or [`js_obj!`].
///
/// **Should not be used directly.**
///
/// [`js_obj!`]:   macro.js_obj.html
/// [`js_int!`]:    macro.js_int.html
/// [`js_double!`]: macro.js_double.html
/// [`js_string!`]: macro.js_string.html
/// [`js!`]:    macro.js.html
#[macro_export]
macro_rules! __js_macro {
    ( $emscr_func:ident, $jscode:expr, $($args:expr),* ) => {
        {
            let jscode : &'static [u8] = format!("{:?}\0", $jscode).as_bytes();
            let arg_sigs: &[u8] = &[$((format!("{:?}", $args), b'd').1 ),*];
            unsafe {
                $crate::emscripten::$emscr_func(jscode as *const _ as *const std::os::raw::c_char, arg_sigs as *const _ as *const std::os::raw::c_char, $( $crate::JSObject::from($args).value ),* )
            }
        }
    };
}

/// Macro that evaluates a JavaScript code snippet with no return value and takes any number of arguments.
///
/// # Arguments
///
/// * `$jscode` - A `&'static str` containing the JavaScript code that needs to be run.
/// * `$args, ...` - Any number of arguments to be used by `$jscode`. All arguments must be of a type `T`
///                  where `std::convert::From<T> for JSObject` is implemented. They can be referenced in
///                  JavaScript snippet as `$0`, `$1`, ...
///
/// Note that the JavaScript snippet is responsible for unpacking the arguments itself using `HELPERJS.loadObject(...)`
/// if the argument is not a simple number of boolean. See the documentation for [`JSObject`] for more information.
///
/// # See also
///
/// For similar macros with different return types, see [`js_int!`], [`js_double!`], [`js_string!`] or [`js_obj!`].
///
/// [`JSObject`]:   struct.JSObject.html
/// [`js_int!`]:    macro.js_int.html
/// [`js_double!`]: macro.js_double.html
/// [`js_string!`]: macro.js_string.html
/// [`js_obj!`]:    macro.js_obj.html
#[macro_export]
macro_rules! js {
    ($jscode:expr $(, $args:expr)*) => {
        __js_macro!(emscripten_asm_const_int, $jscode, $($args),*)
    }
}

/// Macro that evaluates a JavaScript code snippet which returns a JavaScript object.
///
/// # Arguments
///
/// * `$jscode` - A `&'static str` containing the JavaScript code that needs to be run.
/// * `$args, ...` - Any number of arguments to be used by `$jscode`. All arguments must be of a type `T`
///                  where `std::convert::From<T> for JSObject` is implemented. They can be referenced in
///                  JavaScript snippet as `$0`, `$1`, ...
///
/// Note that the JavaScript snippet is responsible for unpacking the arguments itself using `HELPERJS.loadObject(...)`
/// if the argument is not a simple number of boolean. See the documentation for [`JSObject`] for more information.
///
/// # Return value
///
/// An instance of a [`JSObject`] wrapping the return value of the executed JavaScript.
///
/// # See also
///
/// For similar macros with different return types, see [`js_int!`], [`js_double!`], [`js_string!`] or [`js!`].
///
/// [`JSObject`]:   struct.JSObject.html
/// [`js_int!`]:    macro.js_int.html
/// [`js_double!`]: macro.js_double.html
/// [`js_string!`]: macro.js_string.html
/// [`js!`]:    macro.js.html
#[macro_export]
macro_rules! js_obj { // TODO: test
    ($jscode:expr $(, $args:expr )*) => (
        $crate::JSObject {
            value: __js_macro!(emscripten_asm_const_int,
                               concat!("return HELPERJS.storeObject((function(){",
                                       $jscode,
                                       "})();"),
                               $($args),*) as f64,
            jshandle: true,
            refcount: Rc::new(()),
        }
    )
}


/// Macro that evaluates a JavaScript code snippet which returns a string.
///
/// # Arguments
///
/// * `$jscode` - A `&'static str` containing the JavaScript code that needs to be run.
/// * `$args, ...` - Any number of arguments to be used by `$jscode`. All arguments must be of a type `T`
///                  where `std::convert::From<T> for JSObject` is implemented. They can be referenced in
///                  JavaScript snippet as `$0`, `$1`, ...
///
/// Note that the JavaScript snippet is responsible for unpacking the arguments itself using `HELPERJS.loadObject(...)`
/// if the argument is not a simple number of boolean. See the documentation for [`JSObject`] for more information.
///
/// # Return value
///
/// An instance of a `std::string::String` converted from the JavaScript string returned by `$jscode`.
///
/// # See also
///
/// For similar macros with different return types, see [`js_int!`], [`js_double!`], [`js!`] or [`js_obj!`].
///
/// [`JSObject`]:   struct.JSObject.html
/// [`js_int!`]:    macro.js_int.html
/// [`js_double!`]: macro.js_double.html
/// [`js_obj!`]: macro.js_obj.html
/// [`js!`]:    macro.js.html
#[macro_export]
macro_rules! js_string { // TODO: test
    ($jscode:expr $(, $args:expr )*) => (
        String::from(js_obj!($jscode, $($args),*))
    )
}

/// Macro that evaluates a JavaScript code snippet which returns an integer.
///
/// # Arguments
///
/// * `$jscode` - A `&'static str` containing the JavaScript code that needs to be run.
/// * `$args, ...` - Any number of arguments to be used by `$jscode`. All arguments must be of a type `T`
///                  where `std::convert::From<T> for JSObject` is implemented. They can be referenced in
///                  JavaScript snippet as `$0`, `$1`, ...
///
/// Note that the JavaScript snippet is responsible for unpacking the arguments itself using `HELPERJS.loadObject(...)`
/// if the argument is not a simple number of boolean. See the documentation for [`JSObject`] for more information.
///
/// # Return value
///
/// The return value of `$jscode` as an `i32`.
///
/// # See also
///
/// For similar macros with different return types, see [`js!`], [`js_double!`], [`js_string!`] or [`js_obj!`].
///
/// [`JSObject`]:   struct.JSObject.html
/// [`js!`]:    macro.js.html
/// [`js_double!`]: macro.js_double.html
/// [`js_obj!`]: macro.js_obj.html
/// [`js_string!`]: macro.js_string.html
#[macro_export]
macro_rules! js_int {
    ($jscode:expr $(, $args:expr )*) => (
        __js_macro!(emscripten_asm_const_int, $jscode, $($args),*)
    )
}


/// Macro that evaluates a JavaScript code snippet which returns a floating point number.
///
/// # Arguments
///
/// * `$jscode` - A `&'static str` containing the JavaScript code that needs to be run.
/// * `$args, ...` - Any number of arguments to be used by `$jscode`. All arguments must be of a type `T`
///                  where `std::convert::From<T> for JSObject` is implemented. They can be referenced in
///                  JavaScript snippet as `$0`, `$1`, ...
///
/// Note that the JavaScript snippet is responsible for unpacking the arguments itself using `HELPERJS.loadObject(...)`
/// if the argument is not a simple number of boolean. See the documentation for [`JSObject`] for more information.
///
/// # Return value
///
/// The return value of `$jscode` as an `f64`.
///
/// # See also
///
/// For similar macros with different return types, see [`js!`], [`js_double!`], [`js_string!`] or [`js_obj!`].
///
/// [`JSObject`]:   struct.JSObject.html
/// [`js!`]:    macro.js.html
/// [`js_double!`]: macro.js_double.html
/// [`js_obj!`]: macro.js_obj.html
/// [`js_string!`]: macro.js_string.html
#[macro_export]
macro_rules! js_double {
    ($jscode:expr $(, $args:expr )*) => (
        __js_macro!(emscripten_asm_const_double, $jscode, $($args),*)
    )
}


/// A reference to a JavaScript object.
///
/// A JSObject holds a handle to an entry to the [`HELPERJS`] object table.
///
/// The `Drop` implementation for `JSObject` will cause the JavaScript
/// object to be removed from the [`HELPERJS`] object table, allowing it to
/// be reclaimed by the JavaScript garbage collector.
///
/// If you wish to add a type `T` that can be passed to JavaScript, you should
/// implement `std::convert::From<T> for JSObject`.
///
/// # Important note: 
///
/// The `value` field should never be read or modified directly,
/// it is only public so that it can be used by the [`js!`] macro.
///
/// [`HELPERJS`]: index.html#emscripten-helper-global
/// [`js!`]:  macro.js.html
#[derive(Debug, Clone)]
pub struct JSObject {
    pub value: f64,
    jshandle: bool,
    refcount: Rc<()>,
}

impl<'a> std::convert::From<&'a JSObject> for JSObject {
    fn from(v: &'a JSObject) -> Self {
        JSObject::from(v.clone())
    }
}

impl Drop for JSObject {
    fn drop(&mut self) {
        if self.jshandle && Rc::strong_count(&self.refcount) == 1 {
            js!("HELPERJS.releaseObject($0);", self.value);
            
            // let code : &'static [u8] = b"HELPERJS.releaseObject($0);\0";
            // unsafe {
            //     emscripten::emscripten_asm_const_int(code as *const _ as *const std::os::raw::c_char, arg_sigs as *const _ as *const std::os::raw::c_char, self.value);
            // }
        }
    }
}

impl<T> std::convert::From<Vec<T>> for JSObject
    where JSObject: std::convert::From<T> {
    fn from(v: Vec<T>) -> Self {
        let arr = js_obj!("return HELPERJS.storeObject([]);");
        for elem in v {
            let elem_js = JSObject::from(elem);
            let code = if elem_js.jshandle {
                "HELPERJS.loadObject($0).push(HELPERJS.loadObject($1))"
            } else {
                "HELPERJS.loadObject($0).push($1)"
            };

            js!(code, arr.value, elem_js.value);

            // unsafe {
            //     emscripten::emscripten_asm_const_int(code as *const _ as *const std::os::raw::c_char,
            //                                          arr.value, elem_js.value);
            // }
        }
        arr
    }
}

macro_rules! __js_from_numeric {
    ( $( $type:ty ),+ ) => (
        $(
            impl std::convert::From<$type> for JSObject {
                fn from(v: $type) -> Self {
                    JSObject {
                        value: v as f64,
                        jshandle: false,
                        refcount: Rc::new(()),
                    }
                }
            }

            impl std::convert::From<JSObject> for $type {
                fn from(obj: JSObject) -> Self {
                    if obj.jshandle {
                        js_double!("return HELPERJS.loadObject($0);",
                                   obj) as $type
                    } else {
                        obj.value as $type
                    }
                }
            }
        )+
    )
}

__js_from_numeric!(isize, usize, i32, u32, i16, u16, i8, u8, f32, f64);

impl<'a> std::convert::From<&'a str> for JSObject {
    fn from(s: &'a str) -> Self { // TODO: This won't work when the pointer can't fit in 31bit int.
        // let code : &'static [u8] = b"return HELPERJS.storeObject(HELPERJS.copyStringFromHeap($0, $1));\0";
        let data : Vec<u16> = s.encode_utf16().collect();
        let data_ptr_as_isize = data.as_ptr() as isize;
        let value = js_int!("return HELPERJS.storeObject(HELPERJS.copyStringFromHeap($0, $1));", data_ptr_as_isize, data.len() as f64) as f64;
        unsafe {
            JSObject {
                value: value,//emscripten::emscripten_asm_const_int(code as *const _ as *const std::os::raw::c_char, data_ptr_as_isize, data.len()) as f64,
                jshandle: true,
                refcount: Rc::new(()),
            }
        }
    }
}

impl<'a> std::convert::From<&'a String> for JSObject {
    fn from(s: &'a String) -> Self {
        JSObject::from(s.as_str())
    }
}

impl std::convert::From<String> for JSObject {
    fn from(s: String) -> Self {
        JSObject::from(s.as_str())
    }
}

impl std::convert::From<JSObject> for String {
    fn from(obj: JSObject) -> Self {
        let ptr = js_int!("return HELPERJS.storeObject(HELPERJS.copyStringToHeap(HELPERJS.loadObject($0)));",
                          obj) as *mut u16;
        string_from_js(ptr)
    }
}

impl std::convert::From<bool> for JSObject {
    fn from(b: bool) -> Self {
        JSObject {
            value: if b { 1.0 } else { 0.0 },
            jshandle: false,
            refcount: Rc::new(()),
        }
    }
}

impl std::convert::From<JSObject> for bool {
    fn from(obj: JSObject) -> Self {
        if obj.jshandle {
            js_int!("return HELPERJS.loadObject($0);",
                    obj) != 0
        } else {
            obj.value != 0f64
        }
    }
}

/// Initializes the JavaScript [HELPERJS global object and helper functions](index.html#javascript-helpers).
/// Should be called before using any other functions or macros from this crate.
pub fn init() {
    js_eval(concat!(include_str!(concat!(env!("OUT_DIR"), "/helper.js")),
                    "\0").as_bytes())
}

// Rust safe-wrapper using emscripten_set_main_loop_arg
pub fn set_main_loop<F: FnMut() + 'static>(
    fps: std::os::raw::c_int,
    simulate_infinite_loop: std::os::raw::c_int,
    callback: F,
) {
    let on_the_heap = Box::new(callback);
    let leaked_pointer = Box::into_raw(on_the_heap);
    let untyped_pointer = leaked_pointer as *mut std::os::raw::c_void;

    unsafe {
        emscripten::emscripten_set_main_loop_arg(wrapper::<F>, untyped_pointer, fps, simulate_infinite_loop)
    }

    extern "C" fn wrapper<F: FnMut() + 'static>(untyped_pointer: *mut std::os::raw::c_void) {
        let leaked_pointer = untyped_pointer as *mut F;
        let callback_ref = unsafe { &mut *leaked_pointer };
        callback_ref()
    }
}