
extern crate alloc;
use alloc::borrow::Cow;

use bucket_map::{SharedMapping, SharedMappingHandle, WritePermit};
use mork_frontend::bytestring_parser::{Parser, ParserError, Context};
use mork_bytestring::{Expr, ExprZipper, Tag, byte_item, item_byte};
use pathmap::{trie_map::BytesTrieMap, utils::{BitMask, ByteMask}, zipper::*, zipper_tracking::Conflict};

/// The number of S-Expressions returned by [Space::load_sexpr]
pub type SExprCount     = usize;
pub type PathCount      = usize;
pub type AttributeCount = usize;
pub type NodeCount      = usize;
pub type OwnedExpr      = Vec<u8>;
/// A path in the space, expressed in terms of the space's semantic
pub type Path = [u8];

/// Converts the semantic path into a [PathMap] bytes path
pub fn path_as_bytes(path: &Path) -> Cow<[u8]> {
    Cow::from(path)
}

// One should not depend on the string representation of debug as per standard lib. this gives us the room to make these types better later.
#[allow(unused)]
#[derive(Debug)]
pub struct DumpSExprError(String);
#[allow(unused)]
#[derive(Debug)]
pub struct ParseError(String);
#[cfg(feature="neo4j")]
#[allow(unused)]
#[derive(Debug)]
pub struct LoadNeo4JTriplesError(String);
#[cfg(feature="neo4j")]
#[allow(unused)]
#[derive(Debug)]
pub struct LoadNeo4JNodePropertiesError(String);
#[cfg(feature="neo4j")]
#[allow(unused)]
#[derive(Debug)]
pub struct LoadNeo4JNodeLabelsError(String);



pub trait SpaceReaderZipper<'s, 'r> :ZipperMoving + ZipperReadOnlyValues<'s, ()> + ZipperReadOnlySubtries<'s, ()> + ZipperIteration<'s, ()> + ZipperAbsolutePath + 'r {}
impl<'s, 'r, T > SpaceReaderZipper<'s, 'r> for T where T : ZipperMoving + ZipperReadOnlyValues<'s, ()> + ZipperReadOnlySubtries<'s, ()> + ZipperIteration<'s, ()> + ZipperAbsolutePath + 'r {}

/// An interface for accessing the state used by the MORK kernel
pub trait Space {
    /// An authentication token used for access to the space
    type Auth;
    /// Objects of this type encapsulate a location in the space and the rights to read from that location
    type Reader<'space> where Self: 'space;
    /// Objects of this type encapsulate a location in the space and the rights to write to that location
    type Writer<'space> where Self: 'space;
    /// An error type for when a new reader or writer cannot be authenticated.
    type PermissionErr;

    // ===================== Methods used by caller ===================== 

    /// Requests a new [Space::Reader] from the `Space`
    fn new_reader<'space>(&'space self, path: &Path, auth: &Self::Auth) -> Result<Self::Reader<'space>, Self::PermissionErr>;

    /// Requests a new [Space::Writer] from the `Space`
    fn new_writer<'space>(&'space self, path: &Path, auth: &Self::Auth) -> Result<Self::Writer<'space>, Self::PermissionErr>;

    // ===================== Methods used by shared impl ===================== 

    /// Gets a read zipper from a Reader
    ///
    /// NOTE: The `&mut Self::Reader` argument ensures exclusivity, but the `Reader` does
    /// not conceptually have mutable state
    fn read_zipper<'r, 's: 'r>(&'s self, reader: &'r mut Self::Reader<'s>) -> impl SpaceReaderZipper<'s, 'r>;

    /// Gets a write zipper from a Writer
    ///
    /// NOTE: The `&mut Self::Writer` argument ensures exclusivity, but the `Writer` does
    /// not conceptually have mutable state
    fn write_zipper<'w, 's: 'w>(&'s self, writer: &'w mut Self::Writer<'s>) -> impl ZipperMoving + ZipperWriting<()> + 'w;

    /// Returns a handle to the `Space`'s [bucket_map] symbol table.
    fn symbol_table(&self) -> &SharedMappingHandle;

    fn root<'a>(&'a self) -> &'a impl ZipperCreation<'static, ()>;

    /// Parses and loads a buffer of S-Expressions into the `Space`
    ///
    /// Returns the number of expressions loaded into the space
    fn load_sexpr(
        &self,
        src_data : &str,
        pattern  : Expr,
        template : Expr,
        auth     : &Self::Auth,
    ) -> Result<PathCount, Either<ParseError, Either<Conflict, Self::PermissionErr>>> {
        let mut writer_hook = self.writer_hook(auth);
        load_sexpr_impl(
            self,
            &self.symbol_table(),
            |_,p|{
                writer_hook(p).map_err(Either::Right)?;
                self.root().write_zipper_at_exclusive_path(p).map_err(Either::Left)
            },
            src_data,
            pattern,
            template).map_err( |e| match e {
                Either::Left(p) => Either::Left(ParseError(format!("{:?}",p))),
                Either::Right(r) => Either::Right(r),
        })
    }

    #[deprecated]
    /// Parses and loads a buffer of S-Expressions into the `Space`
    ///
    /// Returns the number of expressions loaded into the space
    fn load_sexpr_old<'s>(&'s self, src_data: &str, dst: &mut Self::Writer<'s>) -> Result<SExprCount, ParseError> {
        let mut dst = self.write_zipper(dst);
        load_sexpr_old_impl(self.symbol_table(), src_data, &mut dst).map_err(ParseError)
    }


    fn sexpr_to_expr(&self, sexpr :  &str) -> Result<OwnedExpr, ParseError> {
        sexpr_to_path(self.symbol_table(), sexpr)
    }


    fn dump_as_sexpr<'s, W : std::io::Write>(
        &self,
        writer   : &mut W,
        pattern  : Expr,
        template : Expr,
        auth     : Self::Auth,
    )  -> Result<PathCount, Either<std::io::Error, Either<Conflict, Self::PermissionErr>>> {
        dump_as_sexpr_impl(self.root(), self.symbol_table(), pattern, template, writer, self.reader_hook(&auth))
    }

    #[deprecated]
    fn dump_as_sexpr_old<'s, W : std::io::Write>(&'s self, dst: &mut W,src: &mut Self::Reader<'s>) -> Result<PathCount, DumpSExprError>
    {
        let mut rz = self.read_zipper(src);
        let sm = self.symbol_table();
        dump_as_sexpr_old_impl(sm, dst, &mut rz).map_err(DumpSExprError)
    }

    fn load_csv(
        &self,
        src_data : &str,
        pattern  : Expr,
        template : Expr,
        auth     : &Self::Auth,
    ) -> Result<PathCount, Either<ParseError, Either<Conflict, Self::PermissionErr>>> {
        let mut writer_hook = self.writer_hook(auth);
        load_csv_impl(
            self, 
            &self.symbol_table(), 
                        |_,p|{
                writer_hook(p).map_err(Either::Right)?;
                self.root().write_zipper_at_exclusive_path(p).map_err(Either::Left)
            },
            src_data,
            pattern,
            template,
        ).map_err(Either::Right)
    }

    #[deprecated]
    fn load_csv_old<'s>(&'s self, src_data: &str, dst: &mut Self::Writer<'s>) -> Result<PathCount, ParseError> {
        let sm = self.symbol_table();
        let mut wz = self.write_zipper(dst);
        load_csv_old_impl(sm, &mut wz, src_data).map_err(ParseError)
    }

    fn load_json_old<'s>(&'s self, src_data: &str, dst: &mut Self::Writer<'s>) -> Result<PathCount, ParseError> {
        let mut wz = self.write_zipper(dst);
        let sm = self.symbol_table();
        load_json_impl(sm, &mut wz, src_data).map_err(ParseError)
    }

    fn reader_hook<'a>(&'a self, auth : &'a Self::Auth) -> impl FnMut(&Path)-> Result<(), Self::PermissionErr> + 'a {
        let mut reader_permissions = Vec::new();
        move |p| self.new_reader(p, &auth).map(|w| {reader_permissions.push(w);} )
    }
    fn writer_hook<'a>(&'a self, auth : &'a Self::Auth) -> impl FnMut(&Path)-> Result<(), Self::PermissionErr> + 'a {
        let mut writer_permissions = Vec::new();
        move |p| self.new_writer(p, &auth).map(|w| {writer_permissions.push(w);} )
    }
    fn transform_multi_multi(&self, patterns : &[Expr], templates : &[Expr], auth : Self::Auth) -> Result<(), Either<Conflict, Self::PermissionErr>> {
        transform_multi_multi_impl(self.root(), patterns, templates, self.writer_hook(&auth), self.reader_hook(&auth) )
    }
    fn transform_multi(&self, patterns : &[Expr], template: Expr, auth : Self::Auth) -> Result<(), Either<Conflict, Self::PermissionErr>> {
        self.transform_multi_multi(patterns, &[template], auth)
    }
    fn transform(&self, pattern : Expr, template: Expr, auth : Self::Auth) -> Result<(), Either<Conflict, Self::PermissionErr>> {
        self.transform_multi_multi(&[pattern], &[template], auth)
    }

    #[cfg(feature="neo4j")]
    fn load_neo4j_triples<'s>(&'s self, writer : &mut Self::Writer<'s>, rt : &tokio::runtime::Handle, uri: &str, user: &str, pass: &str) -> Result<PathCount, LoadNeo4JTriplesError> {
        let sm = self.symbol_table();
        let mut wz = self.write_zipper(writer);
        load_neo4j_triples_impl(sm, &mut wz, rt, uri, user, pass).map_err(LoadNeo4JTriplesError)
    }

    #[cfg(feature="neo4j")]
    fn load_neo4j_node_properties<'s>(&'s self, writer : &mut Self::Writer<'s>, rt : &tokio::runtime::Handle, uri: &str, user: &str, pass: &str) -> Result<(NodeCount, AttributeCount), LoadNeo4JNodePropertiesError> {
        let sm = self.symbol_table();
        let mut wz = self.write_zipper(writer);
        load_neo4j_node_properties_impl(sm, &mut wz, rt, uri, user, pass).map_err(LoadNeo4JNodePropertiesError)
    }

    #[cfg(feature="neo4j")]
    fn load_neo4j_node_labels<'s>(&'s self, writer : &mut Self::Writer<'s>, rt : &tokio::runtime::Handle, uri: &str, user: &str, pass: &str) -> Result<(NodeCount, AttributeCount), LoadNeo4JNodeLabelsError> {
        let sm = self.symbol_table();
        let mut wz = self.write_zipper(writer);
        load_neo4j_node_labels_impl(sm, &mut wz, rt, uri, user, pass).map_err(LoadNeo4JNodeLabelsError)
    }
}

/// A default minimalist implementation of [Space]
pub struct DefaultSpace {
    map: ZipperHeadOwned<()>,
    sm: SharedMappingHandle,
}

impl DefaultSpace {
    /// Creates a new empty `DefaultSpace`
    pub fn new() -> Self {
        Self {
            map: BytesTrieMap::new().into_zipper_head([]),
            sm: SharedMapping::new(),
        }
    }
}

impl Space for DefaultSpace {
    type Auth = ();
    type Reader<'space> = ReadZipperTracked<'space, 'static, ()>;
    type Writer<'space> = WriteZipperTracked<'space, 'static, ()>;
    type PermissionErr = String;

    fn new_reader<'space>(&'space self, path: &Path, _auth: &Self::Auth) -> Result<Self::Reader<'space>, Self::PermissionErr> {
        let path = path_as_bytes(path);
        self.map.read_zipper_at_path(path).map_err(|e| e.to_string())
    }
    fn new_writer<'space>(&'space self, path: &Path, _auth: &Self::Auth) -> Result<Self::Writer<'space>, Self::PermissionErr> {
        let path = path_as_bytes(path);
        self.map.write_zipper_at_exclusive_path(path).map_err(|e| e.to_string())
    }
    fn read_zipper<'r, 's: 'r>(&'s self, reader: &'r mut Self::Reader<'s>) -> impl  ZipperMoving + ZipperReadOnlyValues<'s, ()> + ZipperReadOnlySubtries<'s, ()> + ZipperIteration<'s, ()> + ZipperAbsolutePath + 'r {
        reader.reset();
        reader
    }
    fn write_zipper<'w, 's: 'w>(&'s self, writer: &'w mut Self::Writer<'s>) -> impl ZipperMoving + ZipperWriting<()> + 'w {
        writer.reset();
        writer
    }
    fn symbol_table(&self) -> &SharedMappingHandle {
        &self.sm
    }
    fn root<'a>(&'a self) -> &'a impl ZipperCreation<'static, ()> {
        &self.map
    }
}

unsafe extern "C" {
    fn longjmp(env: &mut [u64; 64], status: i32);
    fn setjmp(env: &mut [u64; 64]) -> i32;
}

// this module exists purely to make glob importing it easier
pub(crate) mod stack_actions {
    pub(crate) const ITER_AT_DEPTH    : u8 =  0;
    pub(crate) const ITER_SYMBOL_SIZE : u8 =  1;
    pub(crate) const ITER_SYMBOLS     : u8 =  2;
    pub(crate) const ITER_VARIABLES   : u8 =  3;
    pub(crate) const ITER_ARITIES     : u8 =  4;
    pub(crate) const ITER_EXPR        : u8 =  5;
    pub(crate) const ITER_NESTED      : u8 =  6;
    pub(crate) const ITER_SYMBOL      : u8 =  7;
    pub(crate) const ITER_ARITY       : u8 =  8;
    pub(crate) const ITER_VAR_SYMBOL  : u8 =  9;
    pub(crate) const ITER_VAR_ARITY   : u8 = 10;
    pub(crate) const ACTION           : u8 = 11;
    pub(crate) const BEGIN_RANGE      : u8 = 12;
    pub(crate) const FINALIZE_RANGE   : u8 = 13;
    pub(crate) const REFER_RANGE      : u8 = 14;
    #[allow(unused)]
    pub(crate) const RESERVED         : u8 = 15;

    #[allow(unused)]
    pub(crate) fn label(l: u8) -> String {
        match l {
            ITER_AT_DEPTH    => { "ITER_AT_DEPTH"      }
            ITER_SYMBOL_SIZE => { "ITER_SYMBOL_SIZE"   }
            ITER_SYMBOLS     => { "ITER_SYMBOLS"       }
            ITER_VARIABLES   => { "ITER_VARIABLES"     }
            ITER_ARITIES     => { "ITER_ARITIES"       }
            ITER_EXPR        => { "ITER_EXPR"          }
            ITER_NESTED      => { "ITER_NESTED"        }
            ITER_SYMBOL      => { "ITER_SYMBOL"        }
            ITER_ARITY       => { "ITER_ARITY"         }
            ITER_VAR_SYMBOL  => { "ITER_VAR_SYMBOL"    }
            ITER_VAR_ARITY   => { "ITER_VAR_ARITY"     }
            ACTION           => { "ACTION"             }
            _                => { return l.to_string() }
        }.to_string()
    }
} 
use stack_actions::*;

pub struct ParDataParser<'a> { count: u64,
    #[cfg(feature="interning")]
    buf: [u8; 8],
    #[cfg(not(feature="interning"))]
    buf: [u8; 64],
    #[cfg(not(feature="interning"))]
    truncated: u64,
    #[allow(dead_code)]
    write_permit: WritePermit<'a> }

impl <'a> Parser for ParDataParser<'a> {
    fn tokenizer<'r>(&mut self, s: &[u8]) -> &'r [u8] {
        self.count += 1;
        #[cfg(feature="interning")]
        {
        // FIXME hack until either the parser is rewritten or we can take a pointer of the symbol
        self.buf = self.write_permit.get_sym_or_insert(s);
        return unsafe { std::mem::transmute(&self.buf[..]) };
        }
        #[cfg(not(feature="interning"))]
        {
        let mut l = s.len();
        if l > 63 {
            self.truncated += 1;
            // panic!("len greater than 63 bytes {}", std::str::from_utf8(s).unwrap_or(format!("{:?}", s).as_str()))
            l = 63
        }
        self.buf[..l].clone_from_slice(&s[..l]);
        return unsafe { std::mem::transmute(&self.buf[..l]) };
        }
    }
}

impl <'a> ParDataParser<'a> {
    pub fn new(handle: &'a SharedMappingHandle) -> Self {
        Self {
            count: 3,
            #[cfg(feature="interning")]
            buf: (3u64).to_be_bytes(),
            #[cfg(not(feature="interning"))]
            buf: [0; 64],
            #[cfg(not(feature="interning"))]
            truncated: 0u64,
            write_permit: handle.try_aquire_permission().unwrap()
        }
    }
}


fn referential_transition<Z : ZipperMoving + Zipper + ZipperAbsolutePath, F: FnMut(&[Expr], &mut Z) -> ()>(mut last: *mut u8, loc: &mut Z, references: &mut Vec<Expr>, f: &mut F) {
    unsafe {
    macro_rules! unroll {
    (ACTION $recursive:expr) => { f(&references[..], loc); };
    (ITER_AT_DEPTH $recursive:expr) => {
        let level = *last; last = last.offset(-1);

        let mut i = 0;
        while i < level {
            if loc.descend_first_byte() {
                i += 1
            } else if loc.to_next_sibling_byte() {
            } else if loc.ascend_byte() {
                i -= 1
            } else {
                i = 0;
                break
            }
        }

        while i > 0 {
            if i == level {
                referential_transition(last, loc, references, f);
                if loc.to_next_sibling_byte() {
                } else {
                    assert!(loc.ascend_byte());
                    i -= 1;
                }
            } else if i < level {
                if loc.to_next_sibling_byte() {
                    while i < level && loc.descend_first_byte() {
                        i += 1;
                    }
                } else {
                    assert!(loc.ascend_byte());
                    i -= 1;
                }
            }
        }

        last = last.offset(1); *last = level;
    };
    (ITER_NESTED $recursive:expr) => {
        let arity = *last; last = last.offset(-1);
        if arity == 0 {
          referential_transition(last, loc, references, f);
        } else {
            for _ in 0..arity-1 {
                last = last.offset(1);
                *last = ITER_EXPR;
            }
            unroll!(ITER_EXPR referential_transition(last, loc, references, f));

            last = last.offset(-(arity as isize - 1));
        }
        last = last.offset(1); *last = arity;
    };
    (ITER_SYMBOL_SIZE $recursive:expr) => {
        let m = loc.child_mask().and(&pathmap::utils::ByteMask(SIZES));
        let mut it = m.iter();

        while let Some(b) = it.next() {
            if let Tag::SymbolSize(s) = byte_item(b) {
                let buf = [b];
                if loc.descend_to(buf) {
                    let lastv = *last; last = last.offset(-1);
                    last = last.offset(1); *last = s;
                    last = last.offset(1); *last = lastv;
                    referential_transition(last, loc, references, f);
                    last = last.offset(-1);
                    last = last.offset(-1);
                    last = last.offset(1); *last = lastv;
                }
                loc.ascend(1);
            } else {
                unreachable!("no symbol size next")
            }
        }
    };
    (ITER_SYMBOLS $recursive:expr) => {
         last = last.offset(1); *last = ITER_AT_DEPTH;
         // last = last.offset(1); *last = ITER_SYMBOL_SIZE;
         unroll!(ITER_SYMBOL_SIZE $recursive);
         // last = last.offset(-1);
         last = last.offset(-1);
    };
    (ITER_VARIABLES $recursive:expr) => {
        let m = loc.child_mask().and(&ByteMask(VARS));
        let mut it = m.iter();

        while let Some(b) = it.next() {
            let buf = [b];
            if loc.descend_to(buf) {
                referential_transition(last, loc, references, f);
            }
            loc.ascend(1);
        }
    };
    (ITER_ARITIES $recursive:expr) => {
        let m = loc.child_mask().and(&ByteMask(ARITIES));
        let mut it = m.iter();

        while let Some(b) = it.next() {
            if let Tag::Arity(a) = byte_item(b) {
                let buf = [b];
                if loc.descend_to(buf) {
                    let lastv = *last; last = last.offset(-1);
                    last = last.offset(1); *last = a;
                    last = last.offset(1); *last = lastv;
                    referential_transition(last, loc, references, f);
                    last = last.offset(-1);
                    last = last.offset(-1);
                    last = last.offset(1); *last = lastv;
                }
                loc.ascend(1);
            } else {
                unreachable!()
            }
        }
    };
    (ITER_EXPR $recursive:expr) => {
        unroll!(ITER_VARIABLES $recursive);

        unroll!(ITER_SYMBOLS $recursive);

        last = last.offset(1); *last = ITER_NESTED;
        // last = last.offset(1); *last = ITER_ARITIES;
        unroll!(ITER_ARITIES $recursive);
        // last = last.offset(-1);
        last = last.offset(-1);
    };
    (ITER_SYMBOL $recursive:expr) => {
        let size = *last; last = last.offset(-1);
        let mut v = [0; 64];
        for i in 0..size { *v.get_unchecked_mut(i as usize) = *last; last = last.offset(-1); }

        if loc.descend_to_byte(item_byte(Tag::SymbolSize(size))) {
            if loc.descend_to(&v[..size as usize]) {
                $recursive;
            }
            loc.ascend(size as usize);
        }
        loc.ascend_byte();
        for i in 0..size { last = last.offset(1); *last = *v.get_unchecked((size - i - 1) as usize) }
        last = last.offset(1); *last = size;
    };
    (ITER_VAR_SYMBOL $recursive:expr) => {
        let size = *last; last = last.offset(-1);
        let mut v = [0; 64];
        for i in 0..size { *v.get_unchecked_mut(i as usize) = *last; last = last.offset(-1); }

        unroll!(ITER_VARIABLES $recursive);

        if loc.descend_to_byte(item_byte(Tag::SymbolSize(size))) {
            if loc.descend_to(&v[..size as usize]) {
                referential_transition(last, loc, references, f);
            }
            loc.ascend(size as usize);
        }
        loc.ascend_byte();
        for i in 0..size { last = last.offset(1); *last = *v.get_unchecked((size - i - 1) as usize) }
        last = last.offset(1); *last = size;
    };
    (ITER_ARITY $recursive:expr) => {
        let arity = *last; last = last.offset(-1);
        if loc.descend_to_byte(item_byte(Tag::Arity(arity))) {
            referential_transition(last, loc, references, f);
        }
        loc.ascend_byte();
        last = last.offset(1); *last = arity;
    };
    (ITER_VAR_ARITY $recursive:expr) => {
        let arity = *last; last = last.offset(-1);

        unroll!(ITER_VARIABLES $recursive);

        if loc.descend_to_byte(item_byte(Tag::Arity(arity))) {
            referential_transition(last, loc, references, f);
        }
        loc.ascend_byte();
        last = last.offset(1); *last = arity;
    };
    (BEGIN_RANGE $recursive:expr) => {
        // references.push((loc.path().len() as u32, 0));
        let p = loc.origin_path().unwrap();
        references.push(Expr { ptr: p.as_ptr().cast_mut().offset(p.len() as _) });
        $recursive;
        references.pop();
    };
    (FINALIZE_RANGE $recursive:expr) => {
        // references.last_mut().unwrap().1 = loc.path().len() as u32;
        $recursive;
        // references.last_mut().unwrap().1 = 0;
    };
    (REFER_RANGE $recursive:expr) => {
        let index = *last; last = last.offset(-1);
        let subexpr = references[index as usize];
        let mut ez = ExprZipper::new(subexpr);
        #[allow(unused_mut)]
        let mut v0 = last;
        loop {
            match ez.item() {
                Ok(Tag::NewVar) | Ok(Tag::VarRef(_)) => {
                    last = last.offset(1); *last = ITER_EXPR;
                }
                Ok(Tag::SymbolSize(_)) => { unreachable!() }
                Err(s) => {
                    last = last.offset(1); *last = ITER_VAR_SYMBOL;
                    last = last.offset(1); *last = s.len() as u8;
                    last = last.offset(1);
                    std::ptr::copy_nonoverlapping(s.as_ptr(), last, s.len());
                    last = last.offset((s.len() - 1) as isize);
                }
                Ok(Tag::Arity(a)) => {
                    last = last.offset(1); *last = ITER_VAR_ARITY;
                    last = last.offset(1); *last = a;
                }
            }
            if !ez.next() {
                let d = last.offset_from(v0) as usize;
                std::ptr::slice_from_raw_parts_mut(v0.offset(1), d).as_mut().unwrap_unchecked().reverse();
                break;
            }
        };

        $recursive;
        last = v0;

        last = last.offset(1); *last = index;
    };
    (DISPATCH $s:ident $recursive:expr) => {
        match $s {
            ITER_AT_DEPTH => { unroll!(ITER_AT_DEPTH $recursive); }
            ITER_SYMBOL_SIZE => { unroll!(ITER_SYMBOL_SIZE $recursive); }
            ITER_SYMBOLS => { unroll!(ITER_SYMBOLS $recursive); }
            ITER_VARIABLES => { unroll!(ITER_VARIABLES $recursive); }
            ITER_ARITIES => { unroll!(ITER_ARITIES $recursive); }
            ITER_EXPR => { unroll!(ITER_EXPR $recursive); }
            ITER_NESTED => { unroll!(ITER_NESTED $recursive); }
            ITER_SYMBOL => { unroll!(ITER_SYMBOL $recursive); }
            ITER_ARITY => { unroll!(ITER_ARITY $recursive); }
            ITER_VAR_SYMBOL => { unroll!(ITER_VAR_SYMBOL $recursive); }
            ITER_VAR_ARITY => { unroll!(ITER_VAR_ARITY $recursive); }
            ACTION => { unroll!(ACTION $recursive); }
            BEGIN_RANGE => { unroll!(BEGIN_RANGE $recursive); }
            FINALIZE_RANGE => { unroll!(FINALIZE_RANGE $recursive); }
            REFER_RANGE => { unroll!(REFER_RANGE $recursive); }
            RESERVED => { unreachable!("reserved opcode"); }
            c => { unreachable!("invalid opcode {}", c); }
        }
    };
    (CALL $recursive:expr) => {
        {
            let lastv = *last;
            last = last.offset(-1);
            unroll!(DISPATCH lastv $recursive);
            last = last.offset(1);
            *last = lastv;
        }
    };
    }
    // unroll!(CALL unroll!(CALL unroll!(CALL referential_transition(last, loc, references, f))));
    unroll!(CALL unroll!(CALL referential_transition(last, loc, references, f)));
    // unroll!(CALL referential_transition(last, loc, references, f));
    }
}


pub(crate) const SIZES: [u64; 4] = {
    use mork_bytestring::{item_byte, Tag};

    let mut ret = [0u64; 4];
    let mut size = 1;
    while size < 64 {
        let k = item_byte(Tag::SymbolSize(size));
        ret[((k & 0b11000000) >> 6) as usize] |= 1u64 << (k & 0b00111111);
        size += 1;
    }
    ret
};
pub(crate) const ARITIES: [u64; 4] = {
    use mork_bytestring::{item_byte, Tag};

    let mut ret = [0u64; 4];
    let mut arity = 1;
    while arity < 64 {
        let k = item_byte(Tag::Arity(arity));
        ret[((k & 0b11000000) >> 6) as usize] |= 1u64 << (k & 0b00111111);
        arity += 1;
    }
    ret
};
pub(crate) const VARS: [u64; 4] = {
    use mork_bytestring::{item_byte, Tag};

    let mut ret = [0u64; 4];
    let nv_byte = item_byte(Tag::NewVar);
    ret[((nv_byte & 0b11000000) >> 6) as usize] |= 1u64 << (nv_byte & 0b00111111);
    let mut size = 0;
    while size < 64 {
        let k = item_byte(Tag::VarRef(size));
        ret[((k & 0b11000000) >> 6) as usize] |= 1u64 << (k & 0b00111111);
        size += 1;
    }
    ret
};


#[derive(Debug, Clone, Copy)]
pub enum Either<Left, Right> {
    Left(Left),
    Right(Right),
}
impl<L,R> From<L> for Either<L,R> {
    fn from(value: L) -> Self {
        Either::Left(value)
    }
}

pub(crate) fn transform_multi_multi_impl<'s, Z, HookError>(
    s          : &Z,
    patterns   : &[Expr],
    templates  : &[Expr],
    mut writer_hook : impl FnMut(&Path) -> Result<(), HookError>,
    reader_hook     : impl FnMut(&Path) -> Result<(), HookError>,
) -> Result<(), Either<Conflict, HookError>>
    where 
        Z : ZipperCreation<'s, ()>
{

        let mut buffer = [0u8; 512];
        let template_prefixes: Vec<_> = templates.iter().map(|e| unsafe { e.prefix().unwrap_or_else(|_| e.span()).as_ref().unwrap() }).collect();
        
        let mut template_wzs: Vec<_> = Vec::new();
        for (hook, each) in template_prefixes.iter().map(|p| { (writer_hook(p), s.write_zipper_at_exclusive_path(p))}) {
            hook.map_err(Either::Right)?;
            template_wzs.push(each?)
        }

        query_multi_impl(s, patterns, reader_hook,|refs, _loc| {
            for ((wz, prefix), template) in template_wzs.iter_mut().zip(template_prefixes.iter()).zip(templates.iter()) {
                let mut oz = ExprZipper::new(Expr { ptr: buffer.as_mut_ptr() });
                template.substitute(refs, &mut oz);
                wz.descend_to(&buffer[prefix.len()..oz.loc]);
                wz.set_value(());
                wz.reset();
            }
            Ok(())
        })?;
        drop(template_prefixes);
        Ok(())
}

pub(crate) fn query_multi_impl<'a, Z, HookError, F : FnMut(&[Expr], Expr) -> Result<(), HookError>>
(
    s          : &Z,
    patterns   : &[Expr],
    mut reader_hook : impl FnMut(&Path) -> Result<(), HookError>,
    mut effect : F,
) -> Result<usize, Either<Conflict, HookError>>
where
    Z : ZipperCreation<'a, ()>
{
    let first_pattern_prefix = unsafe { patterns[0].prefix().unwrap_or_else(|_| patterns[0].span()).as_ref().unwrap() };
    
    reader_hook(first_pattern_prefix).map_err(Either::Right)?;
    let mut rz = s.read_zipper_at_path(first_pattern_prefix)?;

    let mut tmp_maps = vec![];
    for p in patterns[1..].iter() {
        tmp_maps.push(BytesTrieMap::new());
        let prefix = unsafe { p.prefix().unwrap_or_else(|_| p.span()).as_ref().unwrap() };
        
        reader_hook(first_pattern_prefix).map_err(Either::Right)?;
        tmp_maps.last_mut().unwrap().write_zipper_at_path(prefix).graft(

            &s.read_zipper_at_path(first_pattern_prefix)?

        );
    }
    rz.descend_to(&[0; 4096]);
    rz.reset();
    let mut prz = ProductZipper::new(rz, patterns[1..].iter().enumerate().map(|(i, p)| {
        let prefix = unsafe { p.prefix().unwrap_or_else(|_| p.span()).as_ref().unwrap() };
        tmp_maps[i].read_zipper_at_path(prefix)
    }));
    prz.descend_to(&[0; 4096]);
    prz.reset();

    let mut stack = vec![0; 1];
    stack[0] = ACTION;

    for pattern in patterns.iter().rev() {
        let prefix = unsafe { pattern.prefix().unwrap_or_else(|_| pattern.span()).as_ref().unwrap() };
        stack.extend_from_slice(&referential_bidirectional_matching_stack_traverse(*pattern, prefix.len())[..]);
    }
    stack.reserve(4096);

    let mut references: Vec<Expr> = vec![];
    let mut candidate = 0;
    // referential_transition(stack.last_mut().unwrap(), &mut prz, &mut references, &mut |refs, loc| {
    //     let e = Expr { ptr: loc.origin_path().unwrap().as_ptr().cast_mut() };
    //     effect(refs, e);
    //     candidate += 1;
    // });
    // Ok(())
    

    thread_local! {
        static BREAK: std::cell::RefCell<[u64; 64]> = const { std::cell::RefCell::new([0; 64]) };
        static RET: std::cell::Cell<*mut u8> = const { std::cell::Cell::new(std::ptr::null_mut()) };
    }
    
    BREAK.with_borrow_mut(|a| {
        if unsafe { setjmp(a) == 0 } {
            referential_transition(stack.last_mut().unwrap(), &mut prz, &mut references, &mut |refs, loc| {
                let e = Expr { ptr: loc.origin_path().unwrap().as_ptr().cast_mut() };
                match effect(refs, e) {
                    Ok(()) => {}
                    Err(t) => {
                        let t_ptr = unsafe { std::alloc::alloc(std::alloc::Layout::new::<HookError>()) };
                        unsafe { std::ptr::write(t_ptr as *mut HookError, t) };
                        RET.set(t_ptr);
                        unsafe { longjmp(a, 1) }
                    }
                }
                unsafe { std::ptr::write_volatile(&mut candidate, std::ptr::read_volatile(&candidate) + 1); }
            })
        }
    });
    RET.with(|mptr| {
        if mptr.get().is_null() { Ok(candidate) }
        else {
            let tref = unsafe { mptr.get() };
            let t = unsafe { std::ptr::read(tref as _) };
            unsafe { std::alloc::dealloc(tref, std::alloc::Layout::new::<HookError>()) };
            Err(t)
        }
    })
}


fn referential_bidirectional_matching_stack_traverse(e: Expr, from: usize) -> Vec<u8> {
    let mut v = mork_bytestring::traverseh!((), (), (Vec<u8>, usize), e, (vec![], from),
        |(v, from): &mut (Vec<u8>, usize), o| {
            if o < *from { return }
            v.push(BEGIN_RANGE);
            v.push(ITER_EXPR);
            v.push(FINALIZE_RANGE);
        },
        |(v, from): &mut (Vec<u8>, usize), o, i| {
            if o < *from { return }
            v.push(REFER_RANGE);
            v.push(i);
        },
        |(v, from): &mut (Vec<u8>, usize), o, s: &[u8]| {
            if o < *from { return }
            v.push(ITER_VAR_SYMBOL);
            v.push(s.len() as u8);
            v.extend(s);
        },
        |(v, from): &mut (Vec<u8>, usize), o, a| {
            if o < *from { return }
            v.push(ITER_VAR_ARITY);
            v.push(a);
        },
        |v, o, r, s| {},
        |v, o, r| {}
    ).0.0;
    v.reverse();
    v
}


// /////////
// SEXPR //
// ///////


pub(crate) fn dump_as_sexpr_impl<'s, Z, W : std::io::Write, HookError>(
    s           : &Z,
    sm          : &SharedMapping,
    pattern     : Expr,
    template    : Expr, 
    w           : &mut W,
    mut reader_hook : impl FnMut(&Path) -> Result<(), HookError>,
) -> Result<usize, Either<std::io::Error, Either<Conflict, HookError>>>
    where
    Z : ZipperCreation<'s, ()>
{
    let mut buffer = [0u8; 4096];

    let q = query_multi_impl(s, &[pattern], |p| reader_hook(p).map_err(Either::Right), |refs, _loc| {
        let mut oz = ExprZipper::new(Expr { ptr: buffer.as_mut_ptr() });
        template.substitute(refs, &mut oz);

        // &buffer[constant_template_prefix.len()..oz.loc]
        Expr{ ptr: buffer.as_ptr().cast_mut() }.serialize(w, |s| {
            #[cfg(feature="interning")]
            {
                let symbol = i64::from_be_bytes(s.try_into().unwrap()).to_be_bytes();
                let mstr = sm.get_bytes(symbol).map(unsafe { |x| std::str::from_utf8_unchecked(x) });
                // println!("symbol {symbol:?}, bytes {mstr:?}");
                unsafe { std::mem::transmute(mstr.expect(format!("failed to look up {:?}", symbol).as_str())) }
            }
            #[cfg(not(feature="interning"))]
            unsafe { std::mem::transmute(std::str::from_utf8(s).unwrap()) }
        });
        w.write(&[b'\n']).map_err(Either::Left)?;

        Ok(())
    });
    q.map_err(|e| { type E<L,R> = Either<L,R>; match e {
        E::Left(conflict)                         => E::Right(E::Left(conflict)),
        E::Right(E::Left(E::Left(io)))            => E::Left(io),
          E::Right(E::Right(hook_error)) 
        | E::Right(E::Left(E::Right(hook_error))) => E::Right(E::Right(hook_error)),
    }})
}

#[deprecated]
pub(crate) fn dump_as_sexpr_old_impl<'s, RZ, W : std::io::Write>(#[allow(unused_variables)]sm : &SharedMappingHandle, dst: &mut W, src: &mut RZ) -> Result<crate::space::PathCount, String> 
    where
    RZ : ZipperIteration<'s, ()>
{
    let mut i = 0;
    loop {
        match src.to_next_val() {
            None => { break }
            Some(()) => {
                let path = src.path();
                let e = Expr { ptr: path.as_ptr().cast_mut() };
                e.serialize(dst, |s| {
                    #[cfg(feature="interning")]
                    {
                    let symbol = i64::from_be_bytes(s.try_into().unwrap()).to_be_bytes();
                    let mstr = sm.get_bytes(symbol).map(unsafe { |x| std::str::from_utf8_unchecked(x) });
                    unsafe { std::mem::transmute(mstr.expect(format!("failed to look up {:?}", symbol).as_str())) }
                    }
                    #[cfg(not(feature="interning"))]
                    unsafe { std::mem::transmute(std::str::from_utf8(s).unwrap()) }
                });
                dst.write(&[b'\n']).map_err(|x| x.to_string())?;
                i += 1;
            }
        }
    }
    Ok(i)
}



pub(crate) fn load_sexpr_impl<'s, S : ?Sized, WZ, HookError>(
    s        : &'s S,
    sm       : &SharedMappingHandle,
    wz_fn    : impl FnOnce(&'s S, &'s [u8]) -> Result<WZ, Either<Conflict, HookError>>,
    r: &str,
    pattern  : Expr,
    template : Expr,
) -> Result<usize, Either<ParserError, Either<Conflict, HookError>>>
where
        WZ : Zipper + ZipperMoving + ZipperWriting<()> + 's
{
    let constant_template_prefix = unsafe { template.prefix().unwrap_or_else(|_| template.span()).as_ref().unwrap() };

    let mut wz = wz_fn(s, constant_template_prefix).map_err(Either::Right)?;
 
    #[allow(unused_mut)]
    let mut buffer = [0u8; 4096];
    let mut it = Context::new(r.as_bytes());
    let mut i = 0;
    let mut stack = [0u8; 2048];
    let mut parser = ParDataParser::new(sm);
    loop {
        let mut ez = ExprZipper::new(Expr{ptr: stack.as_mut_ptr()});
        match parser.sexpr(&mut it, &mut ez) {
            Ok(()) => {
                let data = &stack[..ez.loc];
                let mut oz = ExprZipper::new(Expr{ ptr: buffer.as_ptr().cast_mut() });
                match (Expr{ ptr: data.as_ptr().cast_mut() }.transformData(pattern, template, &mut oz)) {
                    Ok(()) => {}
                    Err(_e) => { continue }
                }
                let new_data = &buffer[..oz.loc];
                wz.descend_to(&new_data[constant_template_prefix.len()..]);
                wz.set_value(());
                wz.reset();
            }
            Err(ParserError::InputFinished) => { break }
            Err(other) => { return Err(Either::Left(other)) }
        }
        i += 1;
        it.variables.clear();
    }
    Ok(i)
}

#[deprecated]
pub(crate) fn load_sexpr_old_impl<'s, WZ, Err>(sm : &SharedMappingHandle, data: &str, dst: &mut WZ) -> Result<SExprCount, Err> 
    where
    WZ : ZipperMoving + ZipperWriting<()>
{
    let mut it = Context::new(data.as_bytes());
    let mut submap = BytesTrieMap::new();

    let mut i = 0;
    let mut stack = [0u8; 2048];
    let mut parser = ParDataParser::new(sm);
    loop {
        let mut ez = ExprZipper::new(Expr{ptr: stack.as_mut_ptr()});
        match parser.sexpr(&mut it, &mut ez) {
            Ok(()) => {
                submap.insert(&stack[..ez.loc], ());
            }
            Err(ParserError::InputFinished) => { break }
            Err(other) => { panic!("{:?}", other) }
        }
        i += 1;
        it.variables.clear();
    }
    dst.graft_map(submap);
    Ok(i)
}

pub(crate) fn sexpr_to_path(sm : &SharedMappingHandle, data: &str) -> Result<OwnedExpr, ParseError> {
    let mut it = Context::new(data.as_bytes());
    let mut stack = [0u8; 2048];
    let mut parser = ParDataParser::new(sm);
    let mut result = None;
    loop {
        let mut ez = ExprZipper::new(Expr{ptr: stack.as_mut_ptr()});
        match parser.sexpr(&mut it, &mut ez) {
            Ok(()) => {
                if result.is_some() {
                    return Err(ParseError(format!("Found multiple S-Expressions in: {data}")))
                }
                result = Some(stack[..ez.loc].to_vec());
            }
            Err(ParserError::InputFinished) => { break }
            Err(other) => { return Err(ParseError(format!("Internal Parse error: {other:?}"))) }
        }
        it.variables.clear();
    }

    result.ok_or_else(|| ParseError(format!("Failed to parse S-Expression: {data}")))
}

// ///////
// CSV //
// /////

#[deprecated]
pub(crate) fn load_csv_old_impl<WZ>(sm : &SharedMappingHandle, wz : &mut WZ, r: &str) -> Result<crate::space::PathCount, String> 
    where
        WZ : Zipper + ZipperMoving + ZipperWriting<()>
{
    wz.reset();

    let mut i = 0;
    let mut stack = [0u8; 2048];
    let mut pdp = ParDataParser::new(sm);
    for sv in r.as_bytes().split(|&x| x == b'\n') {
        if sv.len() == 0 { continue }
        let mut a = 0;
        let e = Expr{ ptr: stack.as_mut_ptr() };
        let mut ez = ExprZipper::new(e);
        ez.loc += 1;
        for symbol in sv.split(|&x| x == b',') {
            let internal = pdp.tokenizer(symbol);
            ez.write_symbol(&internal[..]);
            ez.loc += internal.len() + 1;
            a += 1;
        }
        let total = ez.loc;
        ez.reset();
        ez.write_arity(a);

        // .insert(&stack[..total], ()); // if only we had this function...
        wz.descend_to(&stack[..total]);
        wz.set_value(());
        wz.reset();
        
        i += 1;
    }
    Ok(i)
}

pub(crate) fn load_csv_impl<'s,S:?Sized, WZ, HookError>(
    s        : &'s S, 
    sm       : &SharedMappingHandle,
    wz_fn    : impl FnOnce(&'s S, &'s [u8]) -> Result<WZ, Either<Conflict, HookError>>,
    r        : &str,
    pattern  : Expr,
    template : Expr,
) -> Result<crate::space::PathCount, Either<Conflict, HookError>>
    where
        WZ : Zipper + ZipperMoving + ZipperWriting<()> + 's
{
    let constant_template_prefix = unsafe { template.prefix().unwrap_or_else(|_| template.span()).as_ref().unwrap() };
    
    let mut wz = wz_fn(s, constant_template_prefix)?;

    #[allow(unused_mut)]
    let mut buf = [0u8; 2048];

    let mut i = 0;
    let mut stack = [0u8; 2048];
    let mut pdp = ParDataParser::new(sm);
    for sv in r.as_bytes().split(|&x| x == b'\n') {
        if sv.len() == 0 { continue }
        let mut a = 0;
        let e = Expr{ ptr: stack.as_mut_ptr() };
        let mut ez = ExprZipper::new(e);
        ez.loc += 1;
        for symbol in sv.split(|&x| x == b',') {
            let internal = pdp.tokenizer(symbol);
            ez.write_symbol(&internal[..]);
            ez.loc += internal.len() + 1;
            a += 1;
        }
        let total = ez.loc;
        ez.reset();
        ez.write_arity(a);

        let data = &stack[..total];
        let mut oz = ExprZipper::new(Expr{ ptr: buf.as_ptr().cast_mut() });
        match (Expr{ ptr: data.as_ptr().cast_mut() }.transformData(pattern, template, &mut oz)) {
            Ok(()) => {}
            Err(e) => { continue }
        }
        let new_data = &buf[..oz.loc];
        wz.descend_to(&new_data[constant_template_prefix.len()..]);
        wz.set_value(());
        wz.reset();
        i += 1;
    }

    Ok(i)
}


// ////////
// JSON //
// //////

pub(crate) fn load_json_impl<'s, WZ>(sm : &SharedMappingHandle, wz : &mut WZ, r: &str) -> Result<crate::space::PathCount, String> 
    where 
        WZ : Zipper + ZipperMoving + ZipperWriting<()>
{
    let mut st = SpaceTranscriber{ path_count: 0, wz, pdp: ParDataParser::new(sm) };
    let mut p = crate::json_parser::Parser::new(r);
    p.parse(&mut st).map_err(|e| format!("{e}"))?;
    Ok(st.path_count)
}


pub struct SpaceTranscriber<'a, 'c, WZ> { 
    /// count of unnested values == path_count
    path_count : PathCount, 
    wz         : &'c mut WZ,
    pdp        : ParDataParser<'a> }

impl <'a, 'c, WZ> SpaceTranscriber<'a, 'c, WZ> where WZ : Zipper + ZipperMoving + ZipperWriting<()> {
    #[inline(always)] fn write<S : Into<String>>(&mut self, s: S) {
        use mork_bytestring::{Tag, item_byte};

        let token = self.pdp.tokenizer(s.into().as_bytes());
        let mut path = vec![item_byte(Tag::SymbolSize(token.len() as u8))];
        path.extend(token);
        self.wz.descend_to(&path[..]);
        self.wz.set_value(());
        self.wz.ascend(path.len());
    }
}
impl <'a, 'c, WZ> crate::json_parser::Transcriber for SpaceTranscriber<'a, 'c, WZ> where WZ : Zipper + ZipperMoving + ZipperWriting<()> {
    #[inline(always)] fn descend_index(&mut self, i: usize, first: bool) -> () {
        use mork_bytestring::{Tag, item_byte};

        if first { self.wz.descend_to(&[item_byte(Tag::Arity(2))]); }
        let token = self.pdp.tokenizer(i.to_string().as_bytes());
        self.wz.descend_to(&[item_byte(Tag::SymbolSize(token.len() as u8))]);
        self.wz.descend_to(token);
    }
    #[inline(always)] fn ascend_index(&mut self, i: usize, last: bool) -> () {
        self.wz.ascend(self.pdp.tokenizer(i.to_string().as_bytes()).len() + 1);
        if last { self.wz.ascend(1); }
    }
    #[inline(always)] fn write_empty_array(&mut self) -> () { self.write("[]"); self.path_count += 1; }
    #[inline(always)] fn descend_key(&mut self, k: &str, first: bool) -> () {
        use mork_bytestring::{Tag, item_byte};

        if first { self.wz.descend_to(&[item_byte(Tag::Arity(2))]); }
        let token = self.pdp.tokenizer(k.to_string().as_bytes());
        // let token = k.to_string();
        self.wz.descend_to(&[item_byte(Tag::SymbolSize(token.len() as u8))]);
        self.wz.descend_to(token);
    }
    #[inline(always)] fn ascend_key(&mut self, k: &str, last: bool) -> () {
        let token = self.pdp.tokenizer(k.to_string().as_bytes());
        // let token = k.to_string();
        self.wz.ascend(token.len() + 1);
        if last { self.wz.ascend(1); }
    }
    #[inline(always)] fn write_empty_object(&mut self) -> () { self.write("{}"); self.path_count += 1; }
    #[inline(always)] fn write_string(&mut self, s: &str) -> () { self.write(s); self.path_count += 1; }
    #[inline(always)] fn write_number(&mut self, negative: bool, mantissa: u64, exponent: i16) -> () {
        let mut s = String::new();
        if negative { s.push('-'); }
        s.push_str(mantissa.to_string().as_str());
        if exponent != 0 { s.push('e'); s.push_str(exponent.to_string().as_str()); }
        self.write(s);
        self.path_count += 1;
    }
    #[inline(always)] fn write_true(&mut self) -> () { self.write("true"); self.path_count += 1; }
    #[inline(always)] fn write_false(&mut self) -> () { self.write("false"); self.path_count += 1; }
    #[inline(always)] fn write_null(&mut self) -> () { self.write("null"); self.path_count += 1; }
    #[inline(always)] fn begin(&mut self) -> () {}
    #[inline(always)] fn end(&mut self) -> () {}
}



// /////////
// NEO4J //
// ///////

#[cfg(feature="neo4j")]
pub(crate) fn load_neo4j_triples_impl<'s, WZ>(sm : &SharedMappingHandle, wz : &mut WZ, rt : &tokio::runtime::Handle, uri: &str, user: &str, pass: &str) -> Result<PathCount, String> 
    where
        WZ : Zipper + ZipperMoving + ZipperWriting<()>
{
    use neo4rs::*;

    let graph = Graph::new(uri, user, pass).unwrap();

    let mut pdp = ParDataParser::new(sm);

    let mut count = 0;

    let guard = rt.enter();
    let mut result = rt.block_on(graph.execute(
        query("MATCH (s)-[p]->(o) RETURN id(s), type(p), id(o)"))).unwrap();
    let spo_symbol = pdp.tokenizer("SPO".as_bytes());
    while let Ok(Some(row)) = rt.block_on(result.next()) {
        let s: i64 = row.get("id(s)").unwrap();
        let p: String = row.get("type(p)").unwrap();
        let o: i64 = row.get("id(o)").unwrap();
        let mut buf = [0u8; 64];
        let e = Expr{ ptr: buf.as_mut_ptr() };
        let mut ez = ExprZipper::new(e);
        ez.write_arity(4);
        ez.loc += 1;
        {
            ez.write_symbol(&spo_symbol[..]);
            ez.loc += spo_symbol.len() + 1;
        }
        {
            let internal = pdp.tokenizer(&s.to_be_bytes());
            ez.write_symbol(&internal[..]);
            ez.loc += internal.len() + 1;
        }
        {
            let internal = pdp.tokenizer(&p.as_bytes());
            ez.write_symbol(&internal[..]);
            ez.loc += internal.len() + 1;
        }
        {
            let internal = pdp.tokenizer(&o.to_be_bytes());
            ez.write_symbol(&internal[..]);
            ez.loc += internal.len() + 1;
        }
        // .insert(ez.span(), ()); // if only we had this function...
        wz.descend_to(ez.span());
        wz.set_value(());
        wz.reset();
        
        count += 1;
    }

    drop(guard);
    Ok(count)
}



#[cfg(feature="neo4j")]
pub(crate) fn load_neo4j_node_properties_impl<'s, WZ>(sm : &SharedMappingHandle, wz : &mut WZ, rt : &tokio::runtime::Handle, uri: &str, user: &str, pass: &str) -> Result<(NodeCount, AttributeCount), String> 
    where
        WZ : Zipper + ZipperMoving + ZipperWriting<()>
{
    use neo4rs::*;
    use mork_bytestring::{Tag, item_byte};
    let graph = Graph::new(uri, user, pass).unwrap();

    let mut pdp = ParDataParser::new(sm);
    let sa_symbol = pdp.tokenizer("NKV".as_bytes());
    let mut nodes = 0;
    let mut attributes = 0;

    wz.descend_to_byte(item_byte(Tag::Arity(4)));
    wz.descend_to_byte(item_byte(Tag::SymbolSize(sa_symbol.len() as _)));
    wz.descend_to(sa_symbol);

    let guard = rt.enter();
    let mut result = rt.block_on(graph.execute(
        query("MATCH (s) RETURN id(s), s"))
    ).unwrap();
    while let Ok(Some(row)) = rt.block_on(result.next()) {
        let s: i64 = row.get("id(s)").unwrap();
        let internal_s = pdp.tokenizer(&s.to_be_bytes());
        wz.descend_to_byte(item_byte(Tag::SymbolSize(internal_s.len() as _)));
        wz.descend_to(internal_s);
    
        let a: BoltMap = row.get("s").unwrap();
    
        for (bs, bt) in a.value.iter() {
            let internal_k = pdp.tokenizer(bs.value.as_bytes());
            wz.descend_to_byte(item_byte(Tag::SymbolSize(internal_k.len() as _)));
            wz.descend_to(internal_k);
    
            let BoltType::String(bv) = bt else { unreachable!() };
            if bv.value.starts_with("[") && bv.value.ends_with("]") {
                for chunk in bv.value[1..bv.value.len()-1].split(", ") {
                    let c = if chunk.starts_with("\"") && chunk.ends_with("\"") { &chunk[1..chunk.len()-1] } else { chunk };
                    let internal_v = pdp.tokenizer(c.as_bytes());
                    wz.descend_to_byte(item_byte(Tag::SymbolSize(internal_v.len() as _)));
                    wz.descend_to(internal_v);
    
                    wz.set_value(());
    
                    wz.ascend(internal_v.len() + 1);
                }
            } else {
                let internal_v = pdp.tokenizer(bv.value.as_bytes());
                wz.descend_to_byte(item_byte(Tag::SymbolSize(internal_v.len() as _)));
                wz.descend_to(internal_v);
    
                wz.set_value(());
    
                wz.ascend(internal_v.len() + 1);
            }
    
            wz.ascend(internal_k.len() + 1);
            attributes += 1;
        }
    
        wz.ascend(internal_s.len() + 1);
        nodes += 1;
    }
    drop(guard);
    Ok((nodes, attributes))
}

#[cfg(feature="neo4j")]
pub fn load_neo4j_node_labels_impl<'s, WZ>(sm : &SharedMappingHandle, wz : &mut WZ, rt : &tokio::runtime::Handle, uri: &str, user: &str, pass: &str) -> Result<(usize, usize), String> 
    where
        WZ : Zipper + ZipperMoving + ZipperWriting<()>
{
    use neo4rs::*;
    use mork_bytestring::{Tag, item_byte};
    let graph = Graph::new(uri, user, pass).unwrap();

    let mut pdp = ParDataParser::new(&sm);
    let sa_symbol = pdp.tokenizer("NL".as_bytes());
    let mut nodes = 0;
    let mut labels = 0;

    wz.descend_to_byte(item_byte(Tag::Arity(3)));
    wz.descend_to_byte(item_byte(Tag::SymbolSize(sa_symbol.len() as _)));
    wz.descend_to(sa_symbol);

    let guard = rt.enter();
    let mut result = rt.block_on(graph.execute(
        query("MATCH (s) RETURN id(s), labels(s)"))
    ).unwrap();
    while let Ok(Some(row)) = rt.block_on(result.next()) {
        let s: i64 = row.get("id(s)").unwrap();
        let internal_s = pdp.tokenizer(&s.to_be_bytes());
        wz.descend_to_byte(item_byte(Tag::SymbolSize(internal_s.len() as _)));
        wz.descend_to(internal_s);

        let a: BoltList = row.get("labels(s)").unwrap();

        for bl in a.value.iter() {
            let BoltType::String(bv) = bl else { unreachable!() };

            let internal_v = pdp.tokenizer(bv.value.as_bytes());
            wz.descend_to_byte(item_byte(Tag::SymbolSize(internal_v.len() as _)));
            wz.descend_to(internal_v);

            wz.set_value(());

            wz.ascend(internal_v.len() + 1);

            labels += 1;
        }

        wz.ascend(internal_s.len() + 1);
        nodes += 1;
    }
    drop(guard);
    Ok((nodes, labels))
}