/// Safe wrapper around libclang's C API.
///
/// Provides RAII types for CXIndex, CXTranslationUnit, CXCursor, etc.
/// so callers never need to touch raw pointers directly.
use clang_sys::*;
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;

// ---------------------------------------------------------------------------
// RAII wrappers
// ---------------------------------------------------------------------------

/// Owns a `CXIndex` and disposes it on drop.
pub struct Index {
    raw: CXIndex,
}

impl Index {
    pub fn new() -> Self {
        let raw = unsafe { clang_createIndex(0, 0) };
        assert!(!raw.is_null(), "clang_createIndex returned null");
        Self { raw }
    }

    /// Parse a translation unit from `src` with the given compiler `args`.
    pub fn parse(&self, src: &str, args: &[String]) -> anyhow::Result<TranslationUnit> {
        let c_src = CString::new(src)?;
        let c_args: Vec<CString> = args.iter().map(|a| CString::new(a.as_str()).unwrap()).collect();
        let c_ptrs: Vec<*const i8> = c_args.iter().map(|a| a.as_ptr()).collect();

        let tu = unsafe {
            clang_parseTranslationUnit(
                self.raw,
                c_src.as_ptr(),
                c_ptrs.as_ptr(),
                c_ptrs.len() as i32,
                ptr::null_mut(),
                0,
                CXTranslationUnit_DetailedPreprocessingRecord,
            )
        };
        if tu.is_null() {
            anyhow::bail!("failed to parse translation unit: {src}");
        }
        Ok(TranslationUnit { raw: tu })
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        unsafe { clang_disposeIndex(self.raw) };
    }
}

/// Owns a `CXTranslationUnit` and disposes it on drop.
pub struct TranslationUnit {
    raw: CXTranslationUnit,
}

impl TranslationUnit {
    pub fn cursor(&self) -> Cursor {
        Cursor {
            raw: unsafe { clang_getTranslationUnitCursor(self.raw) },
        }
    }
}

impl Drop for TranslationUnit {
    fn drop(&mut self) {
        unsafe { clang_disposeTranslationUnit(self.raw) };
    }
}

// ---------------------------------------------------------------------------
// Cursor — lightweight, copyable handle
// ---------------------------------------------------------------------------

/// Thin wrapper around `CXCursor`.  Copy-able, no ownership semantics.
#[derive(Clone, Copy)]
pub struct Cursor {
    pub raw: CXCursor,
}

impl Cursor {
    pub fn kind(&self) -> CXCursorKind {
        unsafe { clang_getCursorKind(self.raw) }
    }

    pub fn spelling(&self) -> String {
        cx_string_to_string(unsafe { clang_getCursorSpelling(self.raw) })
    }

    pub fn display_name(&self) -> String {
        cx_string_to_string(unsafe { clang_getCursorDisplayName(self.raw) })
    }

    pub fn usr(&self) -> String {
        cx_string_to_string(unsafe { clang_getCursorUSR(self.raw) })
    }

    pub fn location(&self) -> Location {
        let loc = unsafe { clang_getCursorLocation(self.raw) };
        let mut file: CXFile = ptr::null_mut();
        let mut line: u32 = 0;
        let mut column: u32 = 0;
        unsafe {
            clang_getSpellingLocation(loc, &mut file, &mut line, &mut column, ptr::null_mut());
        }
        let file_name = if file.is_null() {
            None
        } else {
            let s = cx_string_to_string(unsafe { clang_getFileName(file) });
            if s.is_empty() { None } else { Some(s) }
        };
        Location {
            file: file_name,
            line,
            column,
        }
    }

    pub fn semantic_parent(&self) -> Cursor {
        Cursor {
            raw: unsafe { clang_getCursorSemanticParent(self.raw) },
        }
    }

    pub fn referenced(&self) -> Option<Cursor> {
        let r = Cursor {
            raw: unsafe { clang_getCursorReferenced(self.raw) },
        };
        if r.is_null() { None } else { Some(r) }
    }

    pub fn is_null(&self) -> bool {
        unsafe { clang_Cursor_isNull(self.raw) != 0 }
    }

    pub fn is_definition(&self) -> bool {
        unsafe { clang_isCursorDefinition(self.raw) != 0 }
    }

    pub fn cursor_type(&self) -> Type {
        Type {
            raw: unsafe { clang_getCursorType(self.raw) },
        }
    }

    pub fn result_type(&self) -> Type {
        Type {
            raw: unsafe { clang_getCursorResultType(self.raw) },
        }
    }

    pub fn num_arguments(&self) -> i32 {
        unsafe { clang_Cursor_getNumArguments(self.raw) }
    }

    pub fn argument(&self, i: u32) -> Cursor {
        Cursor {
            raw: unsafe { clang_Cursor_getArgument(self.raw, i) },
        }
    }

    pub fn arguments(&self) -> Vec<Cursor> {
        let n = self.num_arguments();
        if n < 0 {
            return Vec::new();
        }
        (0..n as u32).map(|i| self.argument(i)).collect()
    }

    pub fn access_specifier(&self) -> CX_CXXAccessSpecifier {
        unsafe { clang_getCXXAccessSpecifier(self.raw) }
    }

    pub fn is_virtual_method(&self) -> bool {
        unsafe { clang_CXXMethod_isVirtual(self.raw) != 0 }
    }

    pub fn is_pure_virtual_method(&self) -> bool {
        unsafe { clang_CXXMethod_isPureVirtual(self.raw) != 0 }
    }

    pub fn is_static_method(&self) -> bool {
        unsafe { clang_CXXMethod_isStatic(self.raw) != 0 }
    }

    pub fn is_const_method(&self) -> bool {
        unsafe { clang_CXXMethod_isConst(self.raw) != 0 }
    }

    #[cfg(feature = "clang_3_9")]
    pub fn is_default_method(&self) -> bool {
        unsafe { clang_CXXMethod_isDefaulted(self.raw) != 0 }
    }

    #[cfg(not(feature = "clang_3_9"))]
    pub fn is_default_method(&self) -> bool {
        false
    }

    /// Returns overridden cursors for a method.
    pub fn overridden_cursors(&self) -> Vec<Cursor> {
        let mut overridden: *mut CXCursor = ptr::null_mut();
        let mut num: u32 = 0;
        unsafe {
            clang_getOverriddenCursors(self.raw, &mut overridden, &mut num);
        }
        if overridden.is_null() || num == 0 {
            return Vec::new();
        }
        let result: Vec<Cursor> = (0..num)
            .map(|i| Cursor {
                raw: unsafe { *overridden.add(i as usize) },
            })
            .collect();
        unsafe { clang_disposeOverriddenCursors(overridden) };
        result
    }

    /// Visit children, collecting them into a Vec.
    pub fn children(&self) -> Vec<Cursor> {
        let mut children = Vec::new();
        unsafe {
            clang_visitChildren(
                self.raw,
                visit_children_callback,
                &mut children as *mut Vec<Cursor> as *mut std::ffi::c_void,
            );
        }
        children
    }

    pub fn is_translation_unit(&self) -> bool {
        self.kind() == CXCursor_TranslationUnit
    }
}

impl PartialEq for Cursor {
    fn eq(&self, other: &Self) -> bool {
        unsafe { clang_equalCursors(self.raw, other.raw) != 0 }
    }
}
impl Eq for Cursor {}

/// Thin wrapper around `CXType`.
#[derive(Clone, Copy)]
pub struct Type {
    raw: CXType,
}

impl Type {
    pub fn spelling(&self) -> String {
        cx_string_to_string(unsafe { clang_getTypeSpelling(self.raw) }).trim().to_string()
    }
}

/// Source location information.
#[derive(Debug, Clone)]
pub struct Location {
    pub file: Option<String>,
    pub line: u32,
    pub column: u32,
}

// ---------------------------------------------------------------------------
// Compilation database
// ---------------------------------------------------------------------------

pub struct CompilationDatabase {
    raw: CXCompilationDatabase,
}

impl CompilationDatabase {
    pub fn from_directory(dir: &str) -> anyhow::Result<Self> {
        let c_dir = CString::new(dir)?;
        let mut err: CXCompilationDatabase_Error = 0;
        let raw = unsafe { clang_CompilationDatabase_fromDirectory(c_dir.as_ptr(), &mut err) };
        if raw.is_null() || err != 0 {
            anyhow::bail!("failed to load compilation database from {dir}");
        }
        Ok(Self { raw })
    }

    pub fn compile_commands(&self, filename: &str) -> anyhow::Result<CompileCommands> {
        let c_file = CString::new(filename)?;
        let raw = unsafe { clang_CompilationDatabase_getCompileCommands(self.raw, c_file.as_ptr()) };
        if raw.is_null() {
            anyhow::bail!("no compile commands for {filename}");
        }
        Ok(CompileCommands { raw })
    }

    /// Return all compile commands in the database.
    pub fn all_compile_commands(&self) -> anyhow::Result<CompileCommands> {
        let raw = unsafe { clang_CompilationDatabase_getAllCompileCommands(self.raw) };
        if raw.is_null() {
            anyhow::bail!("no compile commands in database");
        }
        Ok(CompileCommands { raw })
    }
}

impl Drop for CompilationDatabase {
    fn drop(&mut self) {
        unsafe { clang_CompilationDatabase_dispose(self.raw) };
    }
}

pub struct CompileCommands {
    raw: CXCompileCommands,
}

impl CompileCommands {
    pub fn len(&self) -> u32 {
        unsafe { clang_CompileCommands_getSize(self.raw) }
    }

    pub fn get(&self, i: u32) -> CompileCommand {
        let raw = unsafe { clang_CompileCommands_getCommand(self.raw, i) };
        CompileCommand { raw }
    }
}

impl Drop for CompileCommands {
    fn drop(&mut self) {
        unsafe { clang_CompileCommands_dispose(self.raw) };
    }
}

pub struct CompileCommand {
    raw: CXCompileCommand,
}

impl CompileCommand {
    pub fn directory(&self) -> String {
        cx_string_to_string(unsafe {
            clang_CompileCommand_getDirectory(self.raw)
        })
    }

    pub fn filename(&self) -> String {
        cx_string_to_string(unsafe {
            clang_CompileCommand_getFilename(self.raw)
        })
    }

    pub fn num_args(&self) -> u32 {
        unsafe { clang_CompileCommand_getNumArgs(self.raw) }
    }

    pub fn arg(&self, i: u32) -> String {
        cx_string_to_string(unsafe {
            clang_CompileCommand_getArg(self.raw, i)
        })
    }

    pub fn arguments(&self) -> Vec<String> {
        (0..self.num_args()).map(|i| self.arg(i)).collect()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn cx_string_to_string(cx: CXString) -> String {
    unsafe {
        let ptr = clang_getCString(cx);
        let s = if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        };
        clang_disposeString(cx);
        s
    }
}

extern "C" fn visit_children_callback(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> CXChildVisitResult {
    let children = unsafe { &mut *(client_data as *mut Vec<Cursor>) };
    children.push(Cursor { raw: cursor });
    CXChildVisit_Continue
}

/// Recursively walk a cursor and all its descendants.
pub fn walk(cursor: Cursor) -> Vec<Cursor> {
    let mut result = vec![cursor];
    for child in cursor.children() {
        result.extend(walk(child));
    }
    result
}

/// Normalize a path to its canonical absolute form.
pub fn norm(p: &str) -> String {
    let path = Path::new(p);
    match path.canonicalize() {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(_) => {
            // fallback: just normalize as best we can
            let expanded = if p.starts_with('~') {
                if let Ok(home) = std::env::var("HOME") {
                    format!("{}{}", home, &p[1..])
                } else {
                    p.to_string()
                }
            } else {
                p.to_string()
            };
            let path = Path::new(&expanded);
            if path.is_absolute() {
                expanded
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(path).to_string_lossy().into_owned())
                    .unwrap_or(expanded)
            }
        }
    }
}
