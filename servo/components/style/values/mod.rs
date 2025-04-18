/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Common [values][values] used in CSS.
//!
//! [values]: https://drafts.csswg.org/css-values/

#![deny(missing_docs)]

use crate::parser::{Parse, ParserContext};
use crate::values::distance::{ComputeSquaredDistance, SquaredDistance};
use crate::Atom;
pub use cssparser::{serialize_identifier, serialize_name, CowRcStr, Parser};
pub use cssparser::{SourceLocation, Token};
use precomputed_hash::PrecomputedHash;
use selectors::parser::SelectorParseErrorKind;
use std::fmt::{self, Debug, Write};
use style_traits::{CssWriter, ParseError, StyleParseErrorKind, ToCss};
use to_shmem::impl_trivial_to_shmem;

#[cfg(feature = "gecko")]
pub use crate::gecko::url::CssUrl;
#[cfg(feature = "servo")]
pub use crate::servo::url::CssUrl;

pub mod animated;
pub mod computed;
pub mod distance;
pub mod generics;
pub mod resolved;
pub mod specified;

/// A CSS float value.
pub type CSSFloat = f32;

/// Normalizes a float value to zero after a set of operations that might turn
/// it into NaN.
#[inline]
pub fn normalize(v: CSSFloat) -> CSSFloat {
    if v.is_nan() {
        0.0
    } else {
        v
    }
}

/// A CSS integer value.
pub type CSSInteger = i32;

/// Serialize an identifier which is represented as an atom.
#[cfg(feature = "gecko")]
pub fn serialize_atom_identifier<W>(ident: &Atom, dest: &mut W) -> fmt::Result
where
    W: Write,
{
    ident.with_str(|s| serialize_identifier(s, dest))
}

/// Serialize an identifier which is represented as an atom.
#[cfg(feature = "servo")]
pub fn serialize_atom_identifier<Static, W>(
    ident: &::string_cache::Atom<Static>,
    dest: &mut W,
) -> fmt::Result
where
    Static: string_cache::StaticAtomSet,
    W: Write,
{
    serialize_identifier(&ident, dest)
}

/// Serialize a name which is represented as an Atom.
#[cfg(feature = "gecko")]
pub fn serialize_atom_name<W>(ident: &Atom, dest: &mut W) -> fmt::Result
where
    W: Write,
{
    ident.with_str(|s| serialize_name(s, dest))
}

/// Serialize a name which is represented as an Atom.
#[cfg(feature = "servo")]
pub fn serialize_atom_name<Static, W>(
    ident: &::string_cache::Atom<Static>,
    dest: &mut W,
) -> fmt::Result
where
    Static: string_cache::StaticAtomSet,
    W: Write,
{
    serialize_name(&ident, dest)
}

/// Serialize a number with calc, and NaN/infinity handling (if enabled)
pub fn serialize_number<W>(v: f32, was_calc: bool, dest: &mut CssWriter<W>) -> fmt::Result
where
    W: Write,
{
    serialize_specified_dimension(v, "", was_calc, dest)
}

/// Serialize a specified dimension with unit, calc, and NaN/infinity handling (if enabled)
pub fn serialize_specified_dimension<W>(
    v: f32,
    unit: &str,
    was_calc: bool,
    dest: &mut CssWriter<W>,
) -> fmt::Result
where
    W: Write,
{
    if was_calc {
        dest.write_str("calc(")?;
    }

    if !v.is_finite() {
        // https://drafts.csswg.org/css-values/#calc-error-constants:
        // "While not technically numbers, these keywords act as numeric values,
        // similar to e and pi. Thus to get an infinite length, for example,
        // requires an expression like calc(infinity * 1px)."

        if v.is_nan() {
            dest.write_str("NaN")?;
        } else if v == f32::INFINITY {
            dest.write_str("infinity")?;
        } else if v == f32::NEG_INFINITY {
            dest.write_str("-infinity")?;
        }

        if !unit.is_empty() {
            dest.write_str(" * 1")?;
        }
    } else {
        v.to_css(dest)?;
    }

    dest.write_str(unit)?;

    if was_calc {
        dest.write_char(')')?;
    }
    Ok(())
}

/// A CSS string stored as an `Atom`.
#[repr(transparent)]
#[derive(
    Clone,
    Debug,
    Default,
    Deref,
    Eq,
    Hash,
    MallocSizeOf,
    PartialEq,
    SpecifiedValueInfo,
    ToComputedValue,
    ToResolvedValue,
    ToShmem,
)]
pub struct AtomString(pub Atom);

#[cfg(feature = "servo")]
impl AsRef<str> for AtomString {
    fn as_ref(&self) -> &str {
        &*self.0
    }
}

impl Parse for AtomString {
    fn parse<'i>(_: &ParserContext, input: &mut Parser<'i, '_>) -> Result<Self, ParseError<'i>> {
        Ok(Self(Atom::from(input.expect_string()?.as_ref())))
    }
}

impl cssparser::ToCss for AtomString {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: Write,
    {
        // Wrap in quotes to form a string literal
        dest.write_char('"')?;
        #[cfg(feature = "servo")]
        {
            cssparser::CssStringWriter::new(dest).write_str(self.as_ref())?;
        }
        #[cfg(feature = "gecko")]
        {
            self.0
                .with_str(|s| cssparser::CssStringWriter::new(dest).write_str(s))?;
        }
        dest.write_char('"')
    }
}

impl style_traits::ToCss for AtomString {
    fn to_css<W>(&self, dest: &mut CssWriter<W>) -> fmt::Result
    where
        W: Write,
    {
        cssparser::ToCss::to_css(self, dest)
    }
}

impl PrecomputedHash for AtomString {
    #[inline]
    fn precomputed_hash(&self) -> u32 {
        self.0.precomputed_hash()
    }
}

impl<'a> From<&'a str> for AtomString {
    #[inline]
    fn from(string: &str) -> Self {
        Self(Atom::from(string))
    }
}

/// A generic CSS `<ident>` stored as an `Atom`.
#[cfg(feature = "servo")]
#[repr(transparent)]
#[derive(Deref)]
pub struct GenericAtomIdent<Set>(pub string_cache::Atom<Set>)
where
    Set: string_cache::StaticAtomSet;

/// A generic CSS `<ident>` stored as an `Atom`, for the default atom set.
#[cfg(feature = "servo")]
pub type AtomIdent = GenericAtomIdent<stylo_atoms::AtomStaticSet>;

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> style_traits::SpecifiedValueInfo for GenericAtomIdent<Set> {}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> Default for GenericAtomIdent<Set> {
    fn default() -> Self {
        Self(string_cache::Atom::default())
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> std::fmt::Debug for GenericAtomIdent<Set> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> std::hash::Hash for GenericAtomIdent<Set> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> Eq for GenericAtomIdent<Set> {}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> PartialEq for GenericAtomIdent<Set> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> Clone for GenericAtomIdent<Set> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> to_shmem::ToShmem for GenericAtomIdent<Set> {
    fn to_shmem(&self, builder: &mut to_shmem::SharedMemoryBuilder) -> to_shmem::Result<Self> {
        use std::mem::ManuallyDrop;

        let atom = self.0.to_shmem(builder)?;
        Ok(ManuallyDrop::new(Self(ManuallyDrop::into_inner(atom))))
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> malloc_size_of::MallocSizeOf for GenericAtomIdent<Set> {
    fn size_of(&self, ops: &mut malloc_size_of::MallocSizeOfOps) -> usize {
        self.0.size_of(ops)
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> cssparser::ToCss for GenericAtomIdent<Set> {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: Write,
    {
        serialize_atom_identifier(&self.0, dest)
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> style_traits::ToCss for GenericAtomIdent<Set> {
    fn to_css<W>(&self, dest: &mut CssWriter<W>) -> fmt::Result
    where
        W: Write,
    {
        serialize_atom_identifier(&self.0, dest)
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> PrecomputedHash for GenericAtomIdent<Set> {
    #[inline]
    fn precomputed_hash(&self) -> u32 {
        self.0.precomputed_hash()
    }
}

#[cfg(feature = "servo")]
impl<'a, Set: string_cache::StaticAtomSet> From<&'a str> for GenericAtomIdent<Set> {
    #[inline]
    fn from(string: &str) -> Self {
        Self(string_cache::Atom::from(string))
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> std::borrow::Borrow<string_cache::Atom<Set>>
    for GenericAtomIdent<Set>
{
    #[inline]
    fn borrow(&self) -> &string_cache::Atom<Set> {
        &self.0
    }
}

#[cfg(feature = "servo")]
impl<Set: string_cache::StaticAtomSet> GenericAtomIdent<Set> {
    /// Constructs a new GenericAtomIdent.
    #[inline]
    pub fn new(atom: string_cache::Atom<Set>) -> Self {
        Self(atom)
    }

    /// Cast an atom ref to an AtomIdent ref.
    #[inline]
    pub fn cast<'a>(atom: &'a string_cache::Atom<Set>) -> &'a Self {
        let ptr = atom as *const _ as *const Self;
        // safety: repr(transparent)
        unsafe { &*ptr }
    }
}

/// A CSS `<ident>` stored as an `Atom`.
#[cfg(feature = "gecko")]
#[repr(transparent)]
#[derive(
    Clone, Debug, Default, Deref, Eq, Hash, MallocSizeOf, PartialEq, SpecifiedValueInfo, ToShmem,
)]
pub struct AtomIdent(pub Atom);

#[cfg(feature = "gecko")]
impl cssparser::ToCss for AtomIdent {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: Write,
    {
        serialize_atom_identifier(&self.0, dest)
    }
}

#[cfg(feature = "gecko")]
impl style_traits::ToCss for AtomIdent {
    fn to_css<W>(&self, dest: &mut CssWriter<W>) -> fmt::Result
    where
        W: Write,
    {
        cssparser::ToCss::to_css(self, dest)
    }
}

#[cfg(feature = "gecko")]
impl PrecomputedHash for AtomIdent {
    #[inline]
    fn precomputed_hash(&self) -> u32 {
        self.0.precomputed_hash()
    }
}

#[cfg(feature = "gecko")]
impl<'a> From<&'a str> for AtomIdent {
    #[inline]
    fn from(string: &str) -> Self {
        Self(Atom::from(string))
    }
}

#[cfg(feature = "gecko")]
impl AtomIdent {
    /// Constructs a new AtomIdent.
    #[inline]
    pub fn new(atom: Atom) -> Self {
        Self(atom)
    }

    /// Like `Atom::with` but for `AtomIdent`.
    pub unsafe fn with<F, R>(ptr: *const crate::gecko_bindings::structs::nsAtom, callback: F) -> R
    where
        F: FnOnce(&Self) -> R,
    {
        Atom::with(ptr, |atom: &Atom| {
            // safety: repr(transparent)
            let atom = atom as *const Atom as *const AtomIdent;
            callback(&*atom)
        })
    }

    /// Cast an atom ref to an AtomIdent ref.
    #[inline]
    pub fn cast<'a>(atom: &'a Atom) -> &'a Self {
        let ptr = atom as *const _ as *const Self;
        // safety: repr(transparent)
        unsafe { &*ptr }
    }
}

#[cfg(feature = "gecko")]
impl std::borrow::Borrow<crate::gecko_string_cache::WeakAtom> for AtomIdent {
    #[inline]
    fn borrow(&self) -> &crate::gecko_string_cache::WeakAtom {
        self.0.borrow()
    }
}

/// Serialize a value into percentage.
pub fn serialize_percentage<W>(value: CSSFloat, dest: &mut CssWriter<W>) -> fmt::Result
where
    W: Write,
{
    serialize_specified_dimension(value * 100., "%", /* was_calc = */ false, dest)
}

/// Serialize a value into normalized (no NaN/inf serialization) percentage.
pub fn serialize_normalized_percentage<W>(value: CSSFloat, dest: &mut CssWriter<W>) -> fmt::Result
where
    W: Write,
{
    (value * 100.).to_css(dest)?;
    dest.write_char('%')
}

/// Convenience void type to disable some properties and values through types.
#[cfg_attr(feature = "servo", derive(Deserialize, MallocSizeOf, Serialize))]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    SpecifiedValueInfo,
    ToAnimatedValue,
    ToComputedValue,
    ToCss,
    ToResolvedValue,
)]
pub enum Impossible {}

// FIXME(nox): This should be derived but the derive code cannot cope
// with uninhabited enums.
impl ComputeSquaredDistance for Impossible {
    #[inline]
    fn compute_squared_distance(&self, _other: &Self) -> Result<SquaredDistance, ()> {
        match *self {}
    }
}

impl_trivial_to_shmem!(Impossible);

impl Parse for Impossible {
    fn parse<'i, 't>(
        _context: &ParserContext,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self, ParseError<'i>> {
        Err(input.new_custom_error(StyleParseErrorKind::UnspecifiedError))
    }
}

/// A struct representing one of two kinds of values.
#[derive(
    Animate,
    Clone,
    ComputeSquaredDistance,
    Copy,
    MallocSizeOf,
    PartialEq,
    Parse,
    SpecifiedValueInfo,
    ToAnimatedValue,
    ToAnimatedZero,
    ToComputedValue,
    ToCss,
    ToResolvedValue,
    ToShmem,
)]
pub enum Either<A, B> {
    /// The first value.
    First(A),
    /// The second kind of value.
    Second(B),
}

impl<A: Debug, B: Debug> Debug for Either<A, B> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Either::First(ref v) => v.fmt(f),
            Either::Second(ref v) => v.fmt(f),
        }
    }
}

/// <https://drafts.csswg.org/css-values-4/#custom-idents>
#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    Hash,
    MallocSizeOf,
    PartialEq,
    SpecifiedValueInfo,
    ToAnimatedValue,
    ToComputedValue,
    ToResolvedValue,
    ToShmem,
)]
#[repr(C)]
pub struct CustomIdent(pub Atom);

impl CustomIdent {
    /// Parse a <custom-ident>
    ///
    /// TODO(zrhoffman, bug 1844501): Use CustomIdent::parse in more places instead of
    /// CustomIdent::from_ident.
    pub fn parse<'i, 't>(
        input: &mut Parser<'i, 't>,
        invalid: &[&str],
    ) -> Result<Self, ParseError<'i>> {
        let location = input.current_source_location();
        let ident = input.expect_ident()?;
        CustomIdent::from_ident(location, ident, invalid)
    }

    /// Parse an already-tokenizer identifier
    pub fn from_ident<'i>(
        location: SourceLocation,
        ident: &CowRcStr<'i>,
        excluding: &[&str],
    ) -> Result<Self, ParseError<'i>> {
        if !Self::is_valid(ident, excluding) {
            return Err(
                location.new_custom_error(SelectorParseErrorKind::UnexpectedIdent(ident.clone()))
            );
        }
        if excluding.iter().any(|s| ident.eq_ignore_ascii_case(s)) {
            Err(location.new_custom_error(StyleParseErrorKind::UnspecifiedError))
        } else {
            Ok(CustomIdent(Atom::from(ident.as_ref())))
        }
    }

    fn is_valid(ident: &str, excluding: &[&str]) -> bool {
        use crate::properties::CSSWideKeyword;
        // https://drafts.csswg.org/css-values-4/#custom-idents:
        //
        //     The CSS-wide keywords are not valid <custom-ident>s. The default
        //     keyword is reserved and is also not a valid <custom-ident>.
        if CSSWideKeyword::from_ident(ident).is_ok() || ident.eq_ignore_ascii_case("default") {
            return false;
        }

        // https://drafts.csswg.org/css-values-4/#custom-idents:
        //
        //     Excluded keywords are excluded in all ASCII case permutations.
        !excluding.iter().any(|s| ident.eq_ignore_ascii_case(s))
    }
}

impl ToCss for CustomIdent {
    fn to_css<W>(&self, dest: &mut CssWriter<W>) -> fmt::Result
    where
        W: Write,
    {
        serialize_atom_identifier(&self.0, dest)
    }
}

/// <https://www.w3.org/TR/css-values-4/#dashed-idents>
/// This is simply an Atom, but will only parse if the identifier starts with "--".
#[repr(transparent)]
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    MallocSizeOf,
    PartialEq,
    SpecifiedValueInfo,
    ToAnimatedValue,
    ToComputedValue,
    ToResolvedValue,
    ToShmem,
    Serialize,
    Deserialize,
)]
pub struct DashedIdent(pub Atom);

impl DashedIdent {
    /// Parse an already-tokenizer identifier
    pub fn from_ident<'i>(
        location: SourceLocation,
        ident: &CowRcStr<'i>,
    ) -> Result<Self, ParseError<'i>> {
        if !ident.starts_with("--") {
            return Err(
                location.new_custom_error(SelectorParseErrorKind::UnexpectedIdent(ident.clone()))
            );
        }
        Ok(Self(Atom::from(ident.as_ref())))
    }

    /// Special value for internal use. Useful where we can't use Option<>.
    pub fn empty() -> Self {
        Self(atom!(""))
    }

    /// Check for special internal value.
    pub fn is_empty(&self) -> bool {
        self.0 == atom!("")
    }
}

impl Parse for DashedIdent {
    fn parse<'i, 't>(
        _: &ParserContext,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self, ParseError<'i>> {
        let location = input.current_source_location();
        let ident = input.expect_ident()?;
        Self::from_ident(location, ident)
    }
}

impl ToCss for DashedIdent {
    fn to_css<W>(&self, dest: &mut CssWriter<W>) -> fmt::Result
    where
        W: Write,
    {
        serialize_atom_identifier(&self.0, dest)
    }
}

/// The <keyframes-name>.
///
/// <https://drafts.csswg.org/css-animations/#typedef-keyframes-name>
///
/// We use a single atom for this. Empty atom represents `none` animation.
#[repr(transparent)]
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    PartialEq,
    MallocSizeOf,
    SpecifiedValueInfo,
    ToComputedValue,
    ToResolvedValue,
    ToShmem,
)]
pub struct KeyframesName(Atom);

impl KeyframesName {
    /// <https://drafts.csswg.org/css-animations/#dom-csskeyframesrule-name>
    pub fn from_ident(value: &str) -> Self {
        Self(Atom::from(value))
    }

    /// Returns the `none` value.
    pub fn none() -> Self {
        Self(atom!(""))
    }

    /// Returns whether this is the special `none` value.
    pub fn is_none(&self) -> bool {
        self.0 == atom!("")
    }

    /// Create a new KeyframesName from Atom.
    #[cfg(feature = "gecko")]
    pub fn from_atom(atom: Atom) -> Self {
        Self(atom)
    }

    /// The name as an Atom
    pub fn as_atom(&self) -> &Atom {
        &self.0
    }
}

impl Parse for KeyframesName {
    fn parse<'i, 't>(
        _: &ParserContext,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self, ParseError<'i>> {
        let location = input.current_source_location();
        Ok(match *input.next()? {
            Token::Ident(ref s) => Self(CustomIdent::from_ident(location, s, &["none"])?.0),
            Token::QuotedString(ref s) => Self(Atom::from(s.as_ref())),
            ref t => return Err(location.new_unexpected_token_error(t.clone())),
        })
    }
}

impl ToCss for KeyframesName {
    fn to_css<W>(&self, dest: &mut CssWriter<W>) -> fmt::Result
    where
        W: Write,
    {
        if self.is_none() {
            return dest.write_str("none");
        }

        fn serialize<W: Write>(string: &str, dest: &mut CssWriter<W>) -> fmt::Result {
            if CustomIdent::is_valid(string, &["none"]) {
                serialize_identifier(string, dest)
            } else {
                string.to_css(dest)
            }
        }

        #[cfg(feature = "gecko")]
        return self.0.with_str(|s| serialize(s, dest));

        #[cfg(feature = "servo")]
        return serialize(self.0.as_ref(), dest);
    }
}
