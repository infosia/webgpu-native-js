#![cfg_attr(not(target_os = "macos"), allow(unused))]

#[cfg(not(target_os = "macos"))]
compile_error!("The jsc-detach spike links the macOS JavaScriptCore framework.");

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CString, c_char, c_void};
    use std::ptr::{self, NonNull};

    enum OpaqueJSContext {}
    enum OpaqueJSValue {}
    enum OpaqueJSString {}
    enum OpaqueJSClass {}

    type JSContextRef = *mut OpaqueJSContext;
    type JSGlobalContextRef = *mut OpaqueJSContext;
    type JSObjectRef = *mut OpaqueJSValue;
    type JSValueRef = *const OpaqueJSValue;
    type JSStringRef = *mut OpaqueJSString;
    type JSClassRef = *const OpaqueJSClass;
    type JSPropertyAttributes = u32;

    const K_JS_PROPERTY_ATTRIBUTE_NONE: JSPropertyAttributes = 0;

    #[link(name = "JavaScriptCore", kind = "framework")]
    unsafe extern "C" {
        fn JSGlobalContextCreate(global_object_class: JSClassRef) -> JSGlobalContextRef;
        fn JSGlobalContextRelease(ctx: JSGlobalContextRef);
        fn JSContextGetGlobalObject(ctx: JSContextRef) -> JSObjectRef;

        fn JSStringCreateWithUTF8CString(string: *const c_char) -> JSStringRef;
        fn JSStringRelease(string: JSStringRef);

        fn JSEvaluateScript(
            ctx: JSContextRef,
            script: JSStringRef,
            this_object: JSObjectRef,
            source_url: JSStringRef,
            starting_line_number: i32,
            exception: *mut JSValueRef,
        ) -> JSValueRef;

        fn JSValueIsUndefined(ctx: JSContextRef, value: JSValueRef) -> bool;
        fn JSValueMakeNumber(ctx: JSContextRef, number: f64) -> JSValueRef;
        fn JSValueToNumber(ctx: JSContextRef, value: JSValueRef, exception: *mut JSValueRef)
        -> f64;
        fn JSValueToObject(
            ctx: JSContextRef,
            value: JSValueRef,
            exception: *mut JSValueRef,
        ) -> JSObjectRef;
        fn JSValueProtect(ctx: JSContextRef, value: JSValueRef);
        fn JSValueUnprotect(ctx: JSContextRef, value: JSValueRef);

        fn JSObjectGetProperty(
            ctx: JSContextRef,
            object: JSObjectRef,
            property_name: JSStringRef,
            exception: *mut JSValueRef,
        ) -> JSValueRef;
        fn JSObjectSetProperty(
            ctx: JSContextRef,
            object: JSObjectRef,
            property_name: JSStringRef,
            value: JSValueRef,
            attributes: JSPropertyAttributes,
            exception: *mut JSValueRef,
        );
        fn JSObjectDeleteProperty(
            ctx: JSContextRef,
            object: JSObjectRef,
            property_name: JSStringRef,
            exception: *mut JSValueRef,
        ) -> bool;
        fn JSObjectGetPropertyAtIndex(
            ctx: JSContextRef,
            object: JSObjectRef,
            property_index: u32,
            exception: *mut JSValueRef,
        ) -> JSValueRef;
        fn JSObjectSetPropertyAtIndex(
            ctx: JSContextRef,
            object: JSObjectRef,
            property_index: u32,
            value: JSValueRef,
            exception: *mut JSValueRef,
        );
        fn JSObjectCallAsFunction(
            ctx: JSContextRef,
            object: JSObjectRef,
            this_object: JSObjectRef,
            argument_count: usize,
            arguments: *const JSValueRef,
            exception: *mut JSValueRef,
        ) -> JSValueRef;

        fn JSObjectGetArrayBufferBytesPtr(
            ctx: JSContextRef,
            object: JSObjectRef,
            exception: *mut JSValueRef,
        ) -> *mut c_void;
        fn JSObjectGetArrayBufferByteLength(
            ctx: JSContextRef,
            object: JSObjectRef,
            exception: *mut JSValueRef,
        ) -> usize;
    }

    /// Error type for the JavaScriptCore detach spike.
    #[derive(Debug, Eq, PartialEq)]
    pub enum Error {
        /// A string passed to JavaScriptCore contained an interior nul byte.
        InteriorNul,
        /// JavaScriptCore returned a null pointer.
        Null(&'static str),
        /// JavaScriptCore reported a JavaScript exception.
        Exception(&'static str),
        /// The requested mapped range is outside the simulated foreign memory.
        RangeOutOfBounds,
        /// The mapped range was already unmapped.
        AlreadyUnmapped,
        /// The requested range is too large for indexed JavaScript property access.
        RangeTooLarge,
    }

    /// Result alias used by the spike.
    pub type Result<T> = std::result::Result<T, Error>;

    struct JsString {
        raw: NonNull<OpaqueJSString>,
    }

    impl JsString {
        fn new(text: &str) -> Result<Self> {
            let c_string = CString::new(text).map_err(|_| Error::InteriorNul)?;
            // SAFETY: JavaScriptCore copies the nul-terminated string before returning.
            let raw = unsafe { JSStringCreateWithUTF8CString(c_string.as_ptr()) };
            let raw = NonNull::new(raw).ok_or(Error::Null("JSStringCreateWithUTF8CString"))?;
            Ok(Self { raw })
        }

        fn as_raw(&self) -> JSStringRef {
            self.raw.as_ptr()
        }
    }

    impl Drop for JsString {
        fn drop(&mut self) {
            // SAFETY: self.raw came from JSStringCreateWithUTF8CString and is released once.
            unsafe { JSStringRelease(self.raw.as_ptr()) };
        }
    }

    /// A single JavaScriptCore global context used by the spike tests.
    pub struct JscContext {
        raw: NonNull<OpaqueJSContext>,
    }

    impl JscContext {
        /// Creates a default JavaScriptCore global context.
        pub fn new() -> Result<Self> {
            // SAFETY: Passing a null class asks JavaScriptCore to create its default global object.
            let raw = unsafe { JSGlobalContextCreate(ptr::null()) };
            let raw = NonNull::new(raw).ok_or(Error::Null("JSGlobalContextCreate"))?;
            Ok(Self { raw })
        }

        fn ctx(&self) -> JSContextRef {
            self.raw.as_ptr()
        }

        fn eval_value(&self, script: &str) -> Result<JSValueRef> {
            let script = JsString::new(script)?;
            let mut exception = ptr::null();
            // SAFETY: All pointers belong to this context; source URL is intentionally absent.
            let value = unsafe {
                JSEvaluateScript(
                    self.ctx(),
                    script.as_raw(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    1,
                    &mut exception,
                )
            };
            if !exception.is_null() {
                return Err(Error::Exception("JSEvaluateScript"));
            }
            if value.is_null() {
                return Err(Error::Null("JSEvaluateScript"));
            }
            Ok(value)
        }

        /// Evaluates JavaScript and converts the result to a number.
        pub fn eval_number(&self, script: &str) -> Result<f64> {
            let value = self.eval_value(script)?;
            let mut exception = ptr::null();
            // SAFETY: value was produced in this context.
            let number = unsafe { JSValueToNumber(self.ctx(), value, &mut exception) };
            if !exception.is_null() {
                return Err(Error::Exception("JSValueToNumber"));
            }
            Ok(number)
        }

        /// Evaluates JavaScript and reports whether the result is `undefined`.
        pub fn eval_is_undefined(&self, script: &str) -> Result<bool> {
            let value = self.eval_value(script)?;
            // SAFETY: value was produced in this context.
            Ok(unsafe { JSValueIsUndefined(self.ctx(), value) })
        }

        /// Sets a global property to the given object.
        pub fn set_global_object(&self, name: &str, object: &MappedRange) -> Result<()> {
            set_global_raw(self.ctx(), name, object.buffer.as_ptr())
        }

        fn engine_array_buffer(&self, byte_length: usize) -> Result<NonNull<OpaqueJSValue>> {
            let value = self.eval_value(&format!("new ArrayBuffer({byte_length})"))?;
            let mut exception = ptr::null();
            // SAFETY: value was created by this context and should be object-convertible.
            let object = unsafe { JSValueToObject(self.ctx(), value, &mut exception) };
            if !exception.is_null() {
                return Err(Error::Exception("JSValueToObject"));
            }
            let object = NonNull::new(object).ok_or(Error::Null("JSValueToObject"))?;
            // SAFETY: Protect keeps the object alive while MappedRange owns the handle.
            unsafe { JSValueProtect(self.ctx(), object.as_ptr().cast_const()) };
            Ok(object)
        }
    }

    fn set_global_raw(ctx: JSContextRef, name: &str, object: JSObjectRef) -> Result<()> {
        let name = JsString::new(name)?;
        let mut exception = ptr::null();
        // SAFETY: The global object and value belong to this context.
        let global = unsafe { JSContextGetGlobalObject(ctx) };
        if global.is_null() {
            return Err(Error::Null("JSContextGetGlobalObject"));
        }
        // SAFETY: All references belong to this context.
        unsafe {
            JSObjectSetProperty(
                ctx,
                global,
                name.as_raw(),
                object.cast_const(),
                K_JS_PROPERTY_ATTRIBUTE_NONE,
                &mut exception,
            );
        }
        if !exception.is_null() {
            return Err(Error::Exception("JSObjectSetProperty"));
        }
        Ok(())
    }

    fn delete_global_raw(ctx: JSContextRef, name: &str) -> Result<()> {
        let name = JsString::new(name)?;
        let mut exception = ptr::null();
        // SAFETY: The global object belongs to this context.
        let global = unsafe { JSContextGetGlobalObject(ctx) };
        if global.is_null() {
            return Err(Error::Null("JSContextGetGlobalObject"));
        }
        // SAFETY: All references belong to this context.
        unsafe {
            JSObjectDeleteProperty(ctx, global, name.as_raw(), &mut exception);
        }
        if !exception.is_null() {
            return Err(Error::Exception("JSObjectDeleteProperty"));
        }
        Ok(())
    }

    struct ProtectedObject {
        ctx: JSContextRef,
        object: NonNull<OpaqueJSValue>,
    }

    impl ProtectedObject {
        fn new(ctx: JSContextRef, object: JSObjectRef) -> Result<Self> {
            let object = NonNull::new(object).ok_or(Error::Null("JSValueToObject"))?;
            // SAFETY: Protect keeps the object alive for C-side indexed access.
            unsafe { JSValueProtect(ctx, object.as_ptr().cast_const()) };
            Ok(Self { ctx, object })
        }

        fn as_raw(&self) -> JSObjectRef {
            self.object.as_ptr()
        }
    }

    impl Drop for ProtectedObject {
        fn drop(&mut self) {
            // SAFETY: object was protected in this context exactly once.
            unsafe { JSValueUnprotect(self.ctx, self.object.as_ptr().cast_const()) };
        }
    }

    fn eval_value_raw(ctx: JSContextRef, script: &str) -> Result<JSValueRef> {
        let script = JsString::new(script)?;
        let mut exception = ptr::null();
        // SAFETY: All pointers belong to this context; source URL is intentionally absent.
        let value = unsafe {
            JSEvaluateScript(
                ctx,
                script.as_raw(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(Error::Exception("JSEvaluateScript"));
        }
        if value.is_null() {
            return Err(Error::Null("JSEvaluateScript"));
        }
        Ok(value)
    }

    impl Drop for JscContext {
        fn drop(&mut self) {
            // SAFETY: self.raw came from JSGlobalContextCreate and is released once.
            unsafe { JSGlobalContextRelease(self.raw.as_ptr()) };
        }
    }

    /// Simulated GPU memory owned by Rust.
    pub struct ForeignMemory {
        bytes: Vec<u8>,
    }

    impl ForeignMemory {
        /// Creates simulated foreign memory using a deterministic byte pattern.
        pub fn patterned(byte_length: usize) -> Self {
            let bytes = (0..byte_length)
                .map(|index| 0xa0_u8.wrapping_add(index as u8))
                .collect();
            Self { bytes }
        }

        /// Returns an immutable view of the foreign bytes.
        pub fn bytes(&self) -> &[u8] {
            &self.bytes
        }

        /// Returns the foreign backing pointer.
        pub fn ptr(&self) -> *const u8 {
            self.bytes.as_ptr()
        }

        /// Mutates one foreign byte from Rust.
        pub fn set(&mut self, index: usize, value: u8) -> Result<()> {
            let Some(slot) = self.bytes.get_mut(index) else {
                return Err(Error::RangeOutOfBounds);
            };
            *slot = value;
            Ok(())
        }
    }

    /// Mapping mode used by `get_mapped_range`.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum MapMode {
        /// Read mapping copies foreign memory into the JS-owned buffer at map time.
        Read,
        /// Write mapping copies the JS-owned buffer back to foreign memory at unmap time.
        Write,
    }

    /// JS-owned mapped range state used by the copy-in/copy-out strategy.
    pub struct MappedRange {
        ctx: JSContextRef,
        buffer: NonNull<OpaqueJSValue>,
        offset: usize,
        byte_length: usize,
        mode: MapMode,
        detached: bool,
    }

    impl MappedRange {
        /// Returns the JavaScript `ArrayBuffer` object for this mapping.
        pub fn object(&self) -> *mut c_void {
            self.buffer.as_ptr().cast()
        }

        /// Returns the buffer byte length according to JavaScriptCore's C API.
        pub fn byte_length(&self) -> Result<usize> {
            let mut exception = ptr::null();
            // SAFETY: buffer is protected and belongs to ctx.
            let len = unsafe {
                JSObjectGetArrayBufferByteLength(self.ctx, self.buffer.as_ptr(), &mut exception)
            };
            if !exception.is_null() {
                return Err(Error::Exception("JSObjectGetArrayBufferByteLength"));
            }
            Ok(len)
        }

        /// Returns JavaScriptCore's temporary pointer for the engine-owned ArrayBuffer.
        pub fn bytes_ptr(&self) -> Result<*mut u8> {
            array_buffer_ptr(self.ctx, self.buffer.as_ptr())
        }

        fn temporary_uint8_view(&self) -> Result<ProtectedObject> {
            set_global_raw(self.ctx, "__mapped_range_buffer", self.buffer.as_ptr())?;
            let value = eval_value_raw(self.ctx, "new Uint8Array(__mapped_range_buffer)")?;
            delete_global_raw(self.ctx, "__mapped_range_buffer")?;
            let mut exception = ptr::null();
            // SAFETY: value was created in this context.
            let object = unsafe { JSValueToObject(self.ctx, value, &mut exception) };
            if !exception.is_null() {
                return Err(Error::Exception("JSValueToObject Uint8Array"));
            }
            ProtectedObject::new(self.ctx, object)
        }

        fn copy_from_foreign(&self, source: &[u8]) -> Result<()> {
            let view = self.temporary_uint8_view()?;
            for (index, byte) in source.iter().copied().enumerate() {
                let index = u32::try_from(index).map_err(|_| Error::RangeTooLarge)?;
                // SAFETY: number creation is infallible for byte values.
                let value = unsafe { JSValueMakeNumber(self.ctx, f64::from(byte)) };
                let mut exception = ptr::null();
                // SAFETY: view is a protected Uint8Array object in this context.
                unsafe {
                    JSObjectSetPropertyAtIndex(
                        self.ctx,
                        view.as_raw(),
                        index,
                        value,
                        &mut exception,
                    );
                }
                if !exception.is_null() {
                    return Err(Error::Exception("JSObjectSetPropertyAtIndex"));
                }
            }
            Ok(())
        }

        fn copy_to_foreign(&self, destination: &mut [u8]) -> Result<()> {
            let view = self.temporary_uint8_view()?;
            for (index, byte) in destination.iter_mut().enumerate() {
                let index = u32::try_from(index).map_err(|_| Error::RangeTooLarge)?;
                let mut exception = ptr::null();
                // SAFETY: view is a protected Uint8Array object in this context.
                let value = unsafe {
                    JSObjectGetPropertyAtIndex(self.ctx, view.as_raw(), index, &mut exception)
                };
                if !exception.is_null() {
                    return Err(Error::Exception("JSObjectGetPropertyAtIndex"));
                }
                if value.is_null() {
                    return Err(Error::Null("JSObjectGetPropertyAtIndex"));
                }
                exception = ptr::null();
                // SAFETY: value is a byte read from a Uint8Array.
                let number = unsafe { JSValueToNumber(self.ctx, value, &mut exception) };
                if !exception.is_null() {
                    return Err(Error::Exception("JSValueToNumber"));
                }
                *byte = number as u8;
            }
            Ok(())
        }

        fn detach(&mut self) -> Result<()> {
            let transfer_name = JsString::new("transfer")?;
            let mut exception = ptr::null();
            // SAFETY: buffer is a live ArrayBuffer object in this context.
            let transfer_value = unsafe {
                JSObjectGetProperty(
                    self.ctx,
                    self.buffer.as_ptr(),
                    transfer_name.as_raw(),
                    &mut exception,
                )
            };
            if !exception.is_null() {
                return Err(Error::Exception("JSObjectGetProperty transfer"));
            }
            if transfer_value.is_null() {
                return Err(Error::Null("ArrayBuffer.prototype.transfer"));
            }
            // SAFETY: transfer_value is expected to be a function object.
            let transfer = unsafe { JSValueToObject(self.ctx, transfer_value, &mut exception) };
            if !exception.is_null() {
                return Err(Error::Exception("JSValueToObject transfer"));
            }
            if transfer.is_null() {
                return Err(Error::Null("JSValueToObject transfer"));
            }
            let call_name = JsString::new("call")?;
            exception = ptr::null();
            // SAFETY: transfer is a function object and inherits Function.prototype.call.
            let call_value = unsafe {
                JSObjectGetProperty(self.ctx, transfer, call_name.as_raw(), &mut exception)
            };
            if !exception.is_null() {
                return Err(Error::Exception("JSObjectGetProperty call"));
            }
            if call_value.is_null() {
                return Err(Error::Null("Function.prototype.call"));
            }
            // SAFETY: call_value is expected to be Function.prototype.call.
            let call = unsafe { JSValueToObject(self.ctx, call_value, &mut exception) };
            if !exception.is_null() {
                return Err(Error::Exception("JSValueToObject call"));
            }
            if call.is_null() {
                return Err(Error::Null("JSValueToObject call"));
            }
            let arguments: [JSValueRef; 1] = [self.buffer.as_ptr().cast_const()];
            exception = ptr::null();
            // SAFETY: This is the C API equivalent of `transfer.call(buffer)`, which transfers
            // the bytes to a new engine-owned ArrayBuffer and detaches the original object.
            let _new_buffer = unsafe {
                JSObjectCallAsFunction(
                    self.ctx,
                    call,
                    transfer,
                    arguments.len(),
                    arguments.as_ptr(),
                    &mut exception,
                )
            };
            if !exception.is_null() {
                return Err(Error::Exception("ArrayBuffer.prototype.transfer"));
            }
            Ok(())
        }
    }

    impl Drop for MappedRange {
        fn drop(&mut self) {
            // SAFETY: buffer was protected in this context exactly once.
            unsafe { JSValueUnprotect(self.ctx, self.buffer.as_ptr().cast_const()) };
        }
    }

    fn array_buffer_ptr(ctx: JSContextRef, object: JSObjectRef) -> Result<*mut u8> {
        let mut exception = ptr::null();
        // SAFETY: object is expected to be an ArrayBuffer owned by this context.
        let ptr = unsafe { JSObjectGetArrayBufferBytesPtr(ctx, object, &mut exception) };
        if !exception.is_null() {
            return Err(Error::Exception("JSObjectGetArrayBufferBytesPtr"));
        }
        Ok(ptr.cast())
    }

    /// Mirrors the JSC `CopyInCopyOut` design for `getMappedRange`.
    pub fn get_mapped_range(
        ctx: &JscContext,
        foreign: &ForeignMemory,
        offset: usize,
        byte_length: usize,
        mode: MapMode,
    ) -> Result<MappedRange> {
        let end = offset
            .checked_add(byte_length)
            .ok_or(Error::RangeOutOfBounds)?;
        let source = foreign
            .bytes
            .get(offset..end)
            .ok_or(Error::RangeOutOfBounds)?;
        let buffer = ctx.engine_array_buffer(byte_length)?;
        let mapped = MappedRange {
            ctx: ctx.ctx(),
            buffer,
            offset,
            byte_length,
            mode,
            detached: false,
        };
        if mode == MapMode::Read {
            mapped.copy_from_foreign(source)?;
        }
        Ok(mapped)
    }

    /// Mirrors the JSC `CopyInCopyOut` design for `unmap`.
    pub fn unmap(mapping: &mut MappedRange, foreign: &mut ForeignMemory) -> Result<()> {
        if mapping.detached {
            return Err(Error::AlreadyUnmapped);
        }
        let end = mapping
            .offset
            .checked_add(mapping.byte_length)
            .ok_or(Error::RangeOutOfBounds)?;
        if mapping.mode == MapMode::Write {
            let destination = foreign
                .bytes
                .get_mut(mapping.offset..end)
                .ok_or(Error::RangeOutOfBounds)?;
            mapping.copy_to_foreign(destination)?;
        } else if foreign.bytes.get(mapping.offset..end).is_none() {
            return Err(Error::RangeOutOfBounds);
        }
        mapping.detach()?;
        mapping.detached = true;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn assert_buffer_contents(ctx: &JscContext, mapping: &MappedRange, expected: &[u8]) {
            ctx.set_global_object("buf", mapping).unwrap();
            for (index, byte) in expected.iter().enumerate() {
                let actual = ctx
                    .eval_number(&format!("new Uint8Array(buf)[{index}]"))
                    .unwrap();
                assert_eq!(actual as u8, *byte);
            }
        }

        #[test]
        fn read_mapping_copies_foreign_pattern_without_exposing_foreign_pointer() {
            let ctx = JscContext::new().unwrap();
            let foreign = ForeignMemory::patterned(16);

            let mapping =
                get_mapped_range(&ctx, &foreign, 0, foreign.bytes().len(), MapMode::Read).unwrap();

            assert_buffer_contents(&ctx, &mapping, foreign.bytes());
            assert_ne!(mapping.bytes_ptr().unwrap().cast_const(), foreign.ptr());
            assert_eq!(mapping.byte_length().unwrap(), foreign.bytes().len());
        }

        #[test]
        fn write_mapping_copies_script_writes_to_foreign_on_unmap() {
            let ctx = JscContext::new().unwrap();
            let mut foreign = ForeignMemory::patterned(8);
            let mut mapping =
                get_mapped_range(&ctx, &foreign, 0, foreign.bytes().len(), MapMode::Write).unwrap();
            ctx.set_global_object("buf", &mapping).unwrap();

            ctx.eval_number(
                "const view = new Uint8Array(buf); \
                 view[0] = 17; view[1] = 34; view[2] = 51; view[3] = 68; view[3]",
            )
            .unwrap();
            unmap(&mut mapping, &mut foreign).unwrap();

            assert_eq!(&foreign.bytes()[0..4], &[17, 34, 51, 68]);
        }

        #[test]
        fn unmap_detaches_stashed_buffer_observably_from_script() {
            let ctx = JscContext::new().unwrap();
            let mut foreign = ForeignMemory::patterned(4);
            let mut mapping =
                get_mapped_range(&ctx, &foreign, 0, foreign.bytes().len(), MapMode::Write).unwrap();
            ctx.set_global_object("buf", &mapping).unwrap();
            ctx.eval_number(
                "globalThis.stash = buf; new Uint8Array(stash)[0] = 99; stash.byteLength",
            )
            .unwrap();

            unmap(&mut mapping, &mut foreign).unwrap();

            assert_eq!(ctx.eval_number("stash.byteLength").unwrap(), 0.0);
            match ctx.eval_value("new Uint8Array(stash)[0]") {
                Err(Error::Exception("JSEvaluateScript")) => {}
                Ok(value) => {
                    // SAFETY: value was produced in this context.
                    assert!(unsafe { JSValueIsUndefined(ctx.ctx(), value) });
                }
                other => panic!("unexpected post-detach read result: {other:?}"),
            }
        }

        #[test]
        fn rust_mutation_after_unmap_is_not_observable_through_stash() {
            let ctx = JscContext::new().unwrap();
            let mut foreign = ForeignMemory::patterned(4);
            let mut mapping =
                get_mapped_range(&ctx, &foreign, 0, foreign.bytes().len(), MapMode::Read).unwrap();
            ctx.set_global_object("buf", &mapping).unwrap();
            ctx.eval_number("globalThis.stash = buf; new Uint8Array(stash)[0]")
                .unwrap();

            unmap(&mut mapping, &mut foreign).unwrap();
            foreign.set(0, 7).unwrap();

            assert_eq!(ctx.eval_number("stash.byteLength").unwrap(), 0.0);
            match ctx.eval_value("new Uint8Array(stash)[0]") {
                Err(Error::Exception("JSEvaluateScript")) => {}
                Ok(value) => {
                    // SAFETY: value was produced in this context.
                    assert!(unsafe { JSValueIsUndefined(ctx.ctx(), value) });
                }
                other => panic!("unexpected post-detach read result: {other:?}"),
            }
        }

        #[test]
        fn direct_script_transfer_detaches_engine_owned_buffer() {
            let ctx = JscContext::new().unwrap();

            let observed = ctx
                .eval_number(
                    "const a = new ArrayBuffer(4); \
                     const b = a.transfer(); \
                     a.byteLength * 100 + b.byteLength",
                )
                .unwrap();

            assert_eq!(observed, 4.0);
        }

        #[test]
        fn unbound_script_transfer_call_detaches_engine_owned_buffer() {
            let ctx = JscContext::new().unwrap();

            let observed = ctx
                .eval_number(
                    "const a = new ArrayBuffer(4); \
                     const f = a.transfer; \
                     const b = f.call(a); \
                     a.byteLength * 100 + b.byteLength",
                )
                .unwrap();

            assert_eq!(observed, 4.0);
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::*;
