use std::cell::RefCell;
use std::ffi::{CStr, CString, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;

#[allow(
    dead_code,
    clippy::upper_case_acronyms,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals
)]
mod qjs {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    InteriorNul,
    Null(&'static str),
    Exception(String),
    EvalReturnedException,
    ToBoolFailed,
    NotArrayBuffer,
    DetachVerificationFailed(&'static str),
    AlreadyUnmapped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreeEvent {
    pub ptr_is_null: bool,
    pub freed_allocation: bool,
}

thread_local! {
    static FREE_EVENTS: RefCell<Vec<FreeEvent>> = const { RefCell::new(Vec::new()) };
}

pub fn take_free_events() -> Vec<FreeEvent> {
    FREE_EVENTS.with(|events| events.take())
}

fn record_free_event(ptr_is_null: bool, freed_allocation: bool) {
    FREE_EVENTS.with(|events| {
        events.borrow_mut().push(FreeEvent {
            ptr_is_null,
            freed_allocation,
        });
    });
}

struct ForeignAllocation {
    bytes: Option<Box<[u8]>>,
}

impl ForeignAllocation {
    fn new(bytes: Vec<u8>) -> Result<(NonNull<u8>, NonNull<Self>)> {
        let mut bytes = bytes.into_boxed_slice();
        let ptr = NonNull::new(bytes.as_mut_ptr()).ok_or(Error::Null("foreign allocation"))?;
        let state = Box::new(Self { bytes: Some(bytes) });
        let state = NonNull::new(Box::into_raw(state)).ok_or(Error::Null("foreign state"))?;
        Ok((ptr, state))
    }
}

unsafe extern "C" fn free_array_buffer(
    _rt: *mut qjs::JSRuntime,
    opaque: *mut c_void,
    ptr: *mut c_void,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if opaque.is_null() {
            record_free_event(ptr.is_null(), false);
            return;
        }

        let state = unsafe { &mut *(opaque.cast::<ForeignAllocation>()) };
        if ptr.is_null() {
            record_free_event(true, false);
            drop(unsafe { Box::from_raw(opaque.cast::<ForeignAllocation>()) });
            return;
        }

        let freed_allocation = state.bytes.take().is_some();
        record_free_event(false, freed_allocation);
    }));
}

pub struct QuickJs {
    rt: NonNull<qjs::JSRuntime>,
    ctx: NonNull<qjs::JSContext>,
}

impl QuickJs {
    pub fn new() -> Result<Self> {
        let rt = unsafe { qjs::JS_NewRuntime() };
        let rt = NonNull::new(rt).ok_or(Error::Null("JS_NewRuntime"))?;
        let ctx = unsafe { qjs::JS_NewContext(rt.as_ptr()) };
        let ctx = NonNull::new(ctx).ok_or(Error::Null("JS_NewContext"))?;
        Ok(Self { rt, ctx })
    }

    fn ctx(&self) -> *mut qjs::JSContext {
        self.ctx.as_ptr()
    }

    pub fn eval_bool(&self, script: &str) -> Result<bool> {
        let value = self.eval_value(script)?;
        let result = unsafe { qjs::JS_ToBool(self.ctx(), value) };
        unsafe { qjs::JS_FreeValue(self.ctx(), value) };
        match result {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::ToBoolFailed),
        }
    }

    pub fn eval_string(&self, script: &str) -> Result<String> {
        let value = self.eval_value(script)?;
        let raw = unsafe { qjs::JS_ToCString(self.ctx(), value) };
        unsafe { qjs::JS_FreeValue(self.ctx(), value) };
        if raw.is_null() {
            return self.take_exception("JS_ToCString");
        }
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { qjs::JS_FreeCString(self.ctx(), raw) };
        Ok(text)
    }

    fn eval_value(&self, script: &str) -> Result<qjs::JSValue> {
        let script = CString::new(script).map_err(|_| Error::InteriorNul)?;
        let filename = c"quickjs-detach-test";
        let value = unsafe {
            qjs::JS_Eval(
                self.ctx(),
                script.as_ptr(),
                script.as_bytes().len(),
                filename.as_ptr(),
                qjs::JS_EVAL_TYPE_GLOBAL as i32,
            )
        };
        if unsafe { qjs::JS_IsException(value) } {
            return self.take_exception("JS_Eval");
        }
        Ok(value)
    }

    fn take_exception<T>(&self, fallback: &'static str) -> Result<T> {
        let exception = unsafe { qjs::JS_GetException(self.ctx()) };
        if unsafe { qjs::JS_IsException(exception) } {
            return Err(Error::EvalReturnedException);
        }
        let raw = unsafe { qjs::JS_ToCString(self.ctx(), exception) };
        let message = if raw.is_null() {
            fallback.to_owned()
        } else {
            let text = unsafe { CStr::from_ptr(raw) }
                .to_string_lossy()
                .into_owned();
            unsafe { qjs::JS_FreeCString(self.ctx(), raw) };
            text
        };
        unsafe { qjs::JS_FreeValue(self.ctx(), exception) };
        Err(Error::Exception(message))
    }

    pub fn mapped_range(&self, bytes: Vec<u8>) -> Result<MappedRange> {
        let (ptr, state) = ForeignAllocation::new(bytes)?;
        let len = unsafe { state.as_ref().bytes.as_ref().map_or(0, |bytes| bytes.len()) };
        let buffer = unsafe {
            qjs::JS_NewArrayBuffer(
                self.ctx(),
                ptr.as_ptr(),
                len,
                Some(free_array_buffer),
                state.as_ptr().cast(),
                false,
            )
        };
        if unsafe { qjs::JS_IsException(buffer) } {
            drop(unsafe { Box::from_raw(state.as_ptr()) });
            return self.take_exception("JS_NewArrayBuffer");
        }
        Ok(MappedRange {
            ctx: self.ctx,
            buffer,
            data: ptr,
            len,
            mapped: true,
        })
    }

    fn detach_and_verify_value(&self, value: qjs::JSValue) -> Result<()> {
        if !unsafe { qjs::JS_IsArrayBuffer(value) } {
            return Err(Error::NotArrayBuffer);
        }

        let mut before_len = 0;
        let before = unsafe { qjs::JS_GetArrayBuffer(self.ctx(), &mut before_len, value) };
        if before.is_null() || before_len == 0 {
            self.clear_pending_exception();
            return Err(Error::DetachVerificationFailed(
                "value was not an attached non-empty ArrayBuffer before detach",
            ));
        }

        unsafe { qjs::JS_DetachArrayBuffer(self.ctx(), value) };

        let mut after_len = usize::MAX;
        let after = unsafe { qjs::JS_GetArrayBuffer(self.ctx(), &mut after_len, value) };
        self.clear_pending_exception();
        if !after.is_null() || after_len != 0 {
            return Err(Error::DetachVerificationFailed(
                "ArrayBuffer remained reachable after detach",
            ));
        }

        let byte_length_is_zero = self.eval_bool("globalThis.__verify.byteLength === 0")?;
        if !byte_length_is_zero {
            return Err(Error::DetachVerificationFailed(
                "ArrayBuffer byteLength did not become zero",
            ));
        }
        Ok(())
    }

    pub fn detach_global_and_verify(&self, global: &str) -> Result<()> {
        let global_name = CString::new(global).map_err(|_| Error::InteriorNul)?;
        let global_obj = unsafe { qjs::JS_GetGlobalObject(self.ctx()) };
        let value = unsafe { qjs::JS_GetPropertyStr(self.ctx(), global_obj, global_name.as_ptr()) };
        unsafe { qjs::JS_FreeValue(self.ctx(), global_obj) };
        if unsafe { qjs::JS_IsException(value) } {
            return self.take_exception("JS_GetPropertyStr");
        }
        self.set_global_value("__verify", value)?;
        let result = self.detach_and_verify_value(value);
        unsafe { qjs::JS_FreeValue(self.ctx(), value) };
        result
    }

    fn set_global_value(&self, name: &str, value: qjs::JSValue) -> Result<()> {
        let name = CString::new(name).map_err(|_| Error::InteriorNul)?;
        let global = unsafe { qjs::JS_GetGlobalObject(self.ctx()) };
        let rc = unsafe {
            qjs::JS_SetPropertyStr(
                self.ctx(),
                global,
                name.as_ptr(),
                qjs::JS_DupValue(self.ctx(), value),
            )
        };
        unsafe { qjs::JS_FreeValue(self.ctx(), global) };
        if rc < 0 {
            return self.take_exception("JS_SetPropertyStr");
        }
        Ok(())
    }

    fn clear_pending_exception(&self) {
        let exception = unsafe { qjs::JS_GetException(self.ctx()) };
        unsafe { qjs::JS_FreeValue(self.ctx(), exception) };
    }

    pub fn run_gc(&self) {
        unsafe { qjs::JS_RunGC(self.rt.as_ptr()) };
    }
}

impl Drop for QuickJs {
    fn drop(&mut self) {
        unsafe {
            qjs::JS_FreeContext(self.ctx.as_ptr());
            qjs::JS_FreeRuntime(self.rt.as_ptr());
        }
    }
}

pub struct MappedRange {
    ctx: NonNull<qjs::JSContext>,
    buffer: qjs::JSValue,
    data: NonNull<u8>,
    len: usize,
    mapped: bool,
}

impl MappedRange {
    pub fn install_global(&self, name: &str) -> Result<()> {
        let name = CString::new(name).map_err(|_| Error::InteriorNul)?;
        let global = unsafe { qjs::JS_GetGlobalObject(self.ctx.as_ptr()) };
        let rc = unsafe {
            qjs::JS_SetPropertyStr(
                self.ctx.as_ptr(),
                global,
                name.as_ptr(),
                qjs::JS_DupValue(self.ctx.as_ptr(), self.buffer),
            )
        };
        unsafe { qjs::JS_FreeValue(self.ctx.as_ptr(), global) };
        if rc < 0 {
            return Err(Error::Exception("JS_SetPropertyStr".to_owned()));
        }
        Ok(())
    }

    pub fn mutate(&mut self, offset: usize, value: u8) -> Result<()> {
        if !self.mapped {
            return Err(Error::AlreadyUnmapped);
        }
        if offset >= self.len {
            return Err(Error::DetachVerificationFailed(
                "mutation offset out of range",
            ));
        }
        unsafe { *self.data.as_ptr().add(offset) = value };
        Ok(())
    }

    pub fn unmap(&mut self) -> Result<()> {
        if !self.mapped {
            return Err(Error::AlreadyUnmapped);
        }
        unsafe { qjs::JS_DetachArrayBuffer(self.ctx.as_ptr(), self.buffer) };
        let mut len = usize::MAX;
        let ptr = unsafe { qjs::JS_GetArrayBuffer(self.ctx.as_ptr(), &mut len, self.buffer) };
        let exception = unsafe { qjs::JS_GetException(self.ctx.as_ptr()) };
        unsafe { qjs::JS_FreeValue(self.ctx.as_ptr(), exception) };
        if !ptr.is_null() || len != 0 {
            return Err(Error::DetachVerificationFailed(
                "mapped ArrayBuffer remained attached after unmap",
            ));
        }
        self.mapped = false;
        Ok(())
    }
}

impl Drop for MappedRange {
    fn drop(&mut self) {
        if self.mapped {
            unsafe { qjs::JS_DetachArrayBuffer(self.ctx.as_ptr(), self.buffer) };
            self.mapped = false;
        }
        unsafe { qjs::JS_FreeValue(self.ctx.as_ptr(), self.buffer) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern() -> Vec<u8> {
        vec![0x11, 0x22, 0x33, 0x44]
    }

    #[test]
    fn zero_copy_script_observes_foreign_mutation() -> Result<()> {
        let _ = take_free_events();
        let js = QuickJs::new()?;
        let mut range = js.mapped_range(pattern())?;
        range.install_global("buf")?;

        assert!(js.eval_bool("new Uint8Array(buf)[1] === 0x22")?);
        range.mutate(1, 0x7f)?;
        assert!(js.eval_bool("new Uint8Array(buf)[1] === 0x7f")?);
        range.unmap()?;
        drop(range);
        drop(js);
        let _ = take_free_events();
        Ok(())
    }

    #[test]
    fn detach_neuters_stashed_buffer_and_view() -> Result<()> {
        let _ = take_free_events();
        let js = QuickJs::new()?;
        let mut range = js.mapped_range(pattern())?;
        range.install_global("buf")?;
        assert!(js.eval_bool(
            "globalThis.stash = buf; globalThis.view = new Uint8Array(buf); view.length === 4 && view[0] === 0x11"
        )?);

        range.unmap()?;

        assert!(js.eval_bool("stash.byteLength === 0")?);
        assert!(js.eval_bool("view.length === 0")?);
        assert!(js.eval_bool("view[0] === undefined")?);
        drop(range);
        drop(js);
        let _ = take_free_events();
        Ok(())
    }

    #[test]
    fn free_func_sequence_is_non_null_then_null_and_frees_once() -> Result<()> {
        let _ = take_free_events();
        let js = QuickJs::new()?;
        let mut range = js.mapped_range(pattern())?;
        range.install_global("buf")?;
        assert!(take_free_events().is_empty());

        range.unmap()?;
        assert_eq!(
            take_free_events(),
            vec![FreeEvent {
                ptr_is_null: false,
                freed_allocation: true
            }]
        );

        drop(range);
        js.run_gc();
        drop(js);
        assert_eq!(
            take_free_events(),
            vec![FreeEvent {
                ptr_is_null: true,
                freed_allocation: false
            }]
        );
        Ok(())
    }

    #[test]
    fn verification_reports_silent_no_ops() -> Result<()> {
        let _ = take_free_events();
        let js = QuickJs::new()?;
        assert!(js.eval_bool("globalThis.notBuffer = ({ byteLength: 4 }); true")?);
        assert_eq!(
            js.detach_global_and_verify("notBuffer"),
            Err(Error::NotArrayBuffer)
        );

        let mut range = js.mapped_range(pattern())?;
        range.install_global("buf")?;
        range.unmap()?;
        assert_eq!(
            js.detach_global_and_verify("buf"),
            Err(Error::DetachVerificationFailed(
                "value was not an attached non-empty ArrayBuffer before detach"
            ))
        );
        drop(range);
        drop(js);
        let _ = take_free_events();
        Ok(())
    }

    #[test]
    fn resizable_array_buffer_detaches_and_retains_max_byte_length() -> Result<()> {
        let _ = take_free_events();
        let js = QuickJs::new()?;
        assert!(js.eval_bool(
            "globalThis.rab = new ArrayBuffer(4, { maxByteLength: 16 }); \
             globalThis.rview = new Uint8Array(rab); \
             rview[0] = 9; rab.resizable === true && rab.byteLength === 4 && rab.maxByteLength === 16"
        )?);

        js.detach_global_and_verify("rab")?;

        assert!(js.eval_bool("rab.byteLength === 0")?);
        assert!(js.eval_bool("rab.maxByteLength === 16")?);
        assert!(js.eval_bool("rab.detached === true")?);
        assert!(js.eval_bool("rview.length === 0")?);
        assert!(js.eval_bool("rview[0] === undefined")?);
        assert_eq!(take_free_events(), Vec::<FreeEvent>::new());
        Ok(())
    }
}
