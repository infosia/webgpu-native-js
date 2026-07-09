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
        fn JSStringGetMaximumUTF8CStringSize(string: JSStringRef) -> usize;
        fn JSStringGetUTF8CString(
            string: JSStringRef,
            buffer: *mut c_char,
            buffer_size: usize,
        ) -> usize;
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
        fn JSValueToStringCopy(
            ctx: JSContextRef,
            value: JSValueRef,
            exception: *mut JSValueRef,
        ) -> JSStringRef;
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
        Exception(String),
        /// The requested mapped range is outside the simulated foreign memory.
        RangeOutOfBounds,
        /// The mapped range was already unmapped.
        AlreadyUnmapped,
        /// Detach silently failed and left an ArrayBuffer attached.
        DetachVerificationFailed(&'static str),
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

        fn to_string_lossy(&self) -> String {
            // SAFETY: self.raw is a live JSStringRef. JavaScriptCore reports the required
            // nul-terminated UTF-8 buffer capacity, including the trailing nul byte.
            let size = unsafe { JSStringGetMaximumUTF8CStringSize(self.raw.as_ptr()) };
            let mut bytes = vec![0_u8; size];
            // SAFETY: bytes has the capacity JavaScriptCore requested.
            let written = unsafe {
                JSStringGetUTF8CString(self.raw.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len())
            };
            if written == 0 {
                return String::new();
            }
            let text_len = written.saturating_sub(1);
            String::from_utf8_lossy(&bytes[..text_len]).into_owned()
        }
    }

    impl Drop for JsString {
        fn drop(&mut self) {
            // SAFETY: self.raw came from JSStringCreateWithUTF8CString and is released once.
            unsafe { JSStringRelease(self.raw.as_ptr()) };
        }
    }

    fn exception_message(
        ctx: JSContextRef,
        exception: JSValueRef,
        fallback: &'static str,
    ) -> Error {
        let mut nested = ptr::null();
        // SAFETY: exception is a JS value produced by this context.
        let text = unsafe { JSValueToStringCopy(ctx, exception, &mut nested) };
        if text.is_null() || !nested.is_null() {
            return Error::Exception(fallback.to_owned());
        }
        let Some(text) = NonNull::new(text) else {
            return Error::Exception(fallback.to_owned());
        };
        let text = JsString { raw: text };
        Error::Exception(text.to_string_lossy())
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
                return Err(exception_message(self.ctx(), exception, "JSEvaluateScript"));
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
                return Err(exception_message(self.ctx(), exception, "JSValueToNumber"));
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
                return Err(exception_message(self.ctx(), exception, "JSValueToObject"));
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
            return Err(exception_message(ctx, exception, "JSObjectSetProperty"));
        }
        Ok(())
    }

    impl Drop for JscContext {
        fn drop(&mut self) {
            // SAFETY: self.raw came from JSGlobalContextCreate and is released once.
            unsafe { JSGlobalContextRelease(self.raw.as_ptr()) };
        }
    }

    struct ProtectedArrayBuffer {
        ctx: JSContextRef,
        object: NonNull<OpaqueJSValue>,
    }

    impl ProtectedArrayBuffer {
        fn new(ctx: JSContextRef, object: NonNull<OpaqueJSValue>) -> Self {
            Self { ctx, object }
        }

        fn as_raw(&self) -> JSObjectRef {
            self.object.as_ptr()
        }
    }

    impl Drop for ProtectedArrayBuffer {
        fn drop(&mut self) {
            // SAFETY: object was protected in this context exactly once.
            unsafe { JSValueUnprotect(self.ctx, self.object.as_ptr().cast_const()) };
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
            array_buffer_len(self.ctx, self.buffer.as_ptr())
        }

        fn copy_to_foreign_from_private(
            &self,
            source: JSObjectRef,
            destination: &mut [u8],
        ) -> Result<()> {
            let source = array_buffer_ptr(self.ctx, source)?;
            if !destination.is_empty() {
                // SAFETY: source points to a private ArrayBuffer returned by transfer(), and
                // destination is a disjoint Rust allocation owned by ForeignMemory.
                unsafe {
                    ptr::copy_nonoverlapping(
                        source.cast_const(),
                        destination.as_mut_ptr(),
                        destination.len(),
                    )
                };
            }
            Ok(())
        }

        fn detach_and_take_private_copy(&mut self) -> Result<NonNull<OpaqueJSValue>> {
            let out = transfer_array_buffer(self.ctx, self.buffer.as_ptr())?;
            if array_buffer_len(self.ctx, self.buffer.as_ptr())? != 0 {
                return Err(Error::DetachVerificationFailed(
                    "ArrayBuffer.prototype.transfer left mapped buffer attached",
                ));
            }
            Ok(out)
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
            return Err(exception_message(
                ctx,
                exception,
                "JSObjectGetArrayBufferBytesPtr",
            ));
        }
        NonNull::new(ptr.cast())
            .map(NonNull::as_ptr)
            .ok_or(Error::Null("JSObjectGetArrayBufferBytesPtr"))
    }

    fn array_buffer_len(ctx: JSContextRef, object: JSObjectRef) -> Result<usize> {
        let mut exception = ptr::null();
        // SAFETY: object is expected to be an ArrayBuffer owned by this context.
        let len = unsafe { JSObjectGetArrayBufferByteLength(ctx, object, &mut exception) };
        if !exception.is_null() {
            return Err(exception_message(
                ctx,
                exception,
                "JSObjectGetArrayBufferByteLength",
            ));
        }
        Ok(len)
    }

    fn transfer_array_buffer(
        ctx: JSContextRef,
        buffer: JSObjectRef,
    ) -> Result<NonNull<OpaqueJSValue>> {
        let transfer_name = JsString::new("transfer")?;
        let mut exception = ptr::null();
        // SAFETY: buffer is a live ArrayBuffer object in this context.
        let transfer_value =
            unsafe { JSObjectGetProperty(ctx, buffer, transfer_name.as_raw(), &mut exception) };
        if !exception.is_null() {
            return Err(exception_message(
                ctx,
                exception,
                "JSObjectGetProperty transfer",
            ));
        }
        if transfer_value.is_null() {
            return Err(Error::Null("ArrayBuffer.prototype.transfer"));
        }
        // SAFETY: transfer_value is expected to be a function object.
        let transfer = unsafe { JSValueToObject(ctx, transfer_value, &mut exception) };
        if !exception.is_null() {
            return Err(exception_message(
                ctx,
                exception,
                "JSValueToObject transfer",
            ));
        }
        if transfer.is_null() {
            return Err(Error::Null("JSValueToObject transfer"));
        }
        let call_name = JsString::new("call")?;
        exception = ptr::null();
        // SAFETY: transfer is a function object and inherits Function.prototype.call.
        let call_value =
            unsafe { JSObjectGetProperty(ctx, transfer, call_name.as_raw(), &mut exception) };
        if !exception.is_null() {
            return Err(exception_message(
                ctx,
                exception,
                "JSObjectGetProperty call",
            ));
        }
        if call_value.is_null() {
            return Err(Error::Null("Function.prototype.call"));
        }
        // SAFETY: call_value is expected to be Function.prototype.call.
        let call = unsafe { JSValueToObject(ctx, call_value, &mut exception) };
        if !exception.is_null() {
            return Err(exception_message(ctx, exception, "JSValueToObject call"));
        }
        if call.is_null() {
            return Err(Error::Null("JSValueToObject call"));
        }
        let arguments: [JSValueRef; 1] = [buffer.cast_const()];
        exception = ptr::null();
        // SAFETY: This is the C API equivalent of `transfer.call(buffer)`.
        let new_buffer = unsafe {
            JSObjectCallAsFunction(
                ctx,
                call,
                transfer,
                arguments.len(),
                arguments.as_ptr(),
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception_message(
                ctx,
                exception,
                "ArrayBuffer.prototype.transfer",
            ));
        }
        let mut exception = ptr::null();
        // SAFETY: ArrayBuffer.prototype.transfer returns an ArrayBuffer object.
        let object = unsafe { JSValueToObject(ctx, new_buffer, &mut exception) };
        if !exception.is_null() {
            return Err(exception_message(
                ctx,
                exception,
                "JSValueToObject transferred ArrayBuffer",
            ));
        }
        NonNull::new(object).ok_or(Error::Null("ArrayBuffer.prototype.transfer result"))
    }

    fn make_visible_buffer(
        ctx: &JscContext,
        source: Option<&[u8]>,
        byte_length: usize,
    ) -> Result<NonNull<OpaqueJSValue>> {
        let staging = ProtectedArrayBuffer::new(ctx.ctx(), ctx.engine_array_buffer(byte_length)?);
        let staging_ptr = array_buffer_ptr(ctx.ctx(), staging.as_raw())?;
        if let Some(source) = source
            && !source.is_empty()
        {
            // SAFETY: staging_ptr points to a private ArrayBuffer that script cannot reach.
            // source is borrowed from ForeignMemory, so the regions cannot overlap.
            unsafe { ptr::copy_nonoverlapping(source.as_ptr(), staging_ptr, source.len()) };
        }
        let visible = transfer_array_buffer(ctx.ctx(), staging.as_raw())?;
        // SAFETY: Protect keeps the visible transferred product alive while MappedRange owns it.
        unsafe { JSValueProtect(ctx.ctx(), visible.as_ptr().cast_const()) };
        Ok(visible)
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
        let buffer =
            make_visible_buffer(ctx, (mode == MapMode::Read).then_some(source), byte_length)?;
        let mapped = MappedRange {
            ctx: ctx.ctx(),
            buffer,
            offset,
            byte_length,
            mode,
            detached: false,
        };
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
        if foreign.bytes.get(mapping.offset..end).is_none() {
            return Err(Error::RangeOutOfBounds);
        }
        let out = mapping.detach_and_take_private_copy()?;
        if mapping.mode == MapMode::Write {
            let destination = foreign
                .bytes
                .get_mut(mapping.offset..end)
                .ok_or(Error::RangeOutOfBounds)?;
            mapping.copy_to_foreign_from_private(out.as_ptr(), destination)?;
        }
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
        fn read_mapping_copies_foreign_pattern_and_hides_foreign_mutation() {
            let ctx = JscContext::new().unwrap();
            let mut foreign = ForeignMemory::patterned(16);

            let mapping =
                get_mapped_range(&ctx, &foreign, 0, foreign.bytes().len(), MapMode::Read).unwrap();
            ctx.set_global_object("buf", &mapping).unwrap();

            assert_buffer_contents(&ctx, &mapping, foreign.bytes());
            foreign.set(1, 0x7f).unwrap();
            assert_eq!(ctx.eval_number("new Uint8Array(buf)[1]").unwrap(), 161.0);
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
                Err(Error::Exception(_)) => {}
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
                Err(Error::Exception(_)) => {}
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

        #[test]
        fn pinned_script_visible_buffer_fails_loudly_on_unmap() {
            let ctx = JscContext::new().unwrap();
            let mut foreign = ForeignMemory::patterned(8);
            let mut mapping =
                get_mapped_range(&ctx, &foreign, 0, foreign.bytes().len(), MapMode::Write).unwrap();
            ctx.set_global_object("buf", &mapping).unwrap();

            let ptr = array_buffer_ptr(mapping.ctx, mapping.buffer.as_ptr()).unwrap();
            assert!(!ptr.is_null());

            assert_eq!(
                unmap(&mut mapping, &mut foreign),
                Err(Error::DetachVerificationFailed(
                    "ArrayBuffer.prototype.transfer left mapped buffer attached"
                ))
            );
            assert_eq!(ctx.eval_number("buf.byteLength").unwrap(), 8.0);
        }

        #[test]
        fn e6_protocol_holds_for_one_mib_ranges() {
            const LEN: usize = 1024 * 1024;
            let ctx = JscContext::new().unwrap();

            let mut read_foreign = ForeignMemory::patterned(LEN);
            let mut read_mapping =
                get_mapped_range(&ctx, &read_foreign, 0, LEN, MapMode::Read).unwrap();
            ctx.set_global_object("readBuf", &read_mapping).unwrap();
            assert_eq!(
                ctx.eval_number(
                    "const r = new Uint8Array(readBuf); \
                     r.length + r[0] + r[65536] + r[1048575]"
                )
                .unwrap(),
                (LEN + 0xa0 + 0xa0 + 0x9f) as f64
            );
            read_foreign.set(65536, 1).unwrap();
            assert_eq!(
                ctx.eval_number("new Uint8Array(readBuf)[65536]").unwrap(),
                160.0
            );
            ctx.eval_number("globalThis.readStash = readBuf; readStash.byteLength")
                .unwrap();
            unmap(&mut read_mapping, &mut read_foreign).unwrap();
            assert_eq!(ctx.eval_number("readStash.byteLength").unwrap(), 0.0);

            let mut write_foreign = ForeignMemory::patterned(LEN);
            let mut write_mapping =
                get_mapped_range(&ctx, &write_foreign, 0, LEN, MapMode::Write).unwrap();
            ctx.set_global_object("writeBuf", &write_mapping).unwrap();
            ctx.eval_number(
                "const w = new Uint8Array(writeBuf); \
                 for (let i = 0; i < w.length; i++) { w[i] = i & 255; } \
                 globalThis.writeStash = writeBuf; w[1048575]",
            )
            .unwrap();
            unmap(&mut write_mapping, &mut write_foreign).unwrap();
            assert_eq!(ctx.eval_number("writeStash.byteLength").unwrap(), 0.0);
            assert_eq!(write_foreign.bytes()[0], 0);
            assert_eq!(write_foreign.bytes()[65536], 0);
            assert_eq!(write_foreign.bytes()[1048575], 255);
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::*;
