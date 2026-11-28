//! Tunable analysis parameters: traits, metadata, visitor pattern, and pipeline aggregation.
//! Subsystems declare their own parameter groups via [`param_group!`]; this module re-exports
//! them.
//! 
//! Every parameter group in the program is collected by [`AnalysisParams`].

use serde::{Deserialize, Serialize};

pub use crate::contour::ContourParams;
pub use crate::projection2d::RelaxParams;
pub use crate::interface::cavities::CavityParams;
pub use crate::interface::uv_mesh::InterfaceMeshParams;
pub use crate::interface::GeometryParams;
pub use crate::physics::ContactParams;
pub use crate::sasa::SasaParams;
pub use crate::surface::SurfaceParams;

/// Default value of a parameter, matching its scalar type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamDefault {
    F32(f32),
    Usize(usize),
    Bool(bool),
    /// Index into a list of choices e.g. an Enum or Vec
    Choice(usize),
}

/// Conversion of a value into a [`ParamDefault`].
pub trait IntoParamDefault {
    fn into_param_default(self) -> ParamDefault;
}

impl IntoParamDefault for f32 {
    fn into_param_default(self) -> ParamDefault {
        ParamDefault::F32(self)
    }
}

impl IntoParamDefault for usize {
    fn into_param_default(self) -> ParamDefault {
        ParamDefault::Usize(self)
    }
}

impl IntoParamDefault for bool {
    fn into_param_default(self) -> ParamDefault {
        ParamDefault::Bool(self)
    }
}

impl<T: ParamChoice> IntoParamDefault for T {
    fn into_param_default(self) -> ParamDefault {
        ParamDefault::Choice(self.index())
    }
}

/// An enum usable as a parameter choice, with stable index. Declared with [`param_choice!`].
pub trait ParamChoice: Copy + PartialEq + 'static {
    /// Options in declaration order (index 0..N).
    const OPTIONS: &'static [ChoiceOption];
    fn index(self) -> usize;
    fn from_index(i: usize) -> Option<Self>;
}

/// One selectable option of a [`ParamChoice`] enum.
#[derive(Clone, Copy, Debug)]
pub struct ChoiceOption {
    /// Serialized key, e.g. "atom_surfaces".
    pub key: &'static str,
    /// Human-facing display name, e.g. "Atom surfaces".
    pub name: &'static str,
    /// Didactic user-facing description of this option.
    pub description: &'static str,
}

/// Static description of a single tunable value, passed to a [`ParamVisitor`].
#[derive(Clone, Copy, Debug)]
pub struct ParamMeta {
    /// Serialized key and code identifier, e.g. "probe_radius_a".
    pub key: &'static str,
    /// Human-facing display name, e.g. "Probe radius".
    pub name: &'static str,
    /// Measurement unit (e.g. "Å", "Å²", "°", "1/Å", or "" if dimensionless).
    pub unit: &'static str,
    /// Didactic user-facing description or docstring.
    pub description: &'static str,
    /// Suggested minimum value (enforced by UI, but not on load).
    pub min: Option<f64>,
    /// Suggested maximum value (enforced by UI, but not on load).
    pub max: Option<f64>,
    /// Default value for this parameter.
    pub default: ParamDefault,
}

impl ParamMeta {
    pub fn is_default_f32(&self, val: f32) -> bool {
        match self.default {
            ParamDefault::F32(d) => val == d,
            _ => false,
        }
    }

    pub fn is_default_usize(&self, val: usize) -> bool {
        match self.default {
            ParamDefault::Usize(d) => val == d,
            _ => false,
        }
    }

    pub fn is_default_bool(&self, val: bool) -> bool {
        match self.default {
            ParamDefault::Bool(d) => val == d,
            _ => false,
        }
    }

    pub fn reset_f32(&self, val: &mut f32) {
        if let ParamDefault::F32(d) = self.default {
            *val = d;
        }
    }

    pub fn reset_usize(&self, val: &mut usize) {
        if let ParamDefault::Usize(d) = self.default {
            *val = d;
        }
    }

    pub fn reset_bool(&self, val: &mut bool) {
        if let ParamDefault::Bool(d) = self.default {
            *val = d;
        }
    }
}

/// Receives each field of a [`ParamGroup`]; implemented by editors and serializers.
pub trait ParamVisitor {
    fn visit_f32(&mut self, meta: &ParamMeta, value: &mut f32);
    fn visit_usize(&mut self, meta: &ParamMeta, value: &mut usize);
    fn visit_bool(&mut self, meta: &ParamMeta, value: &mut bool);
    /// Receives a choice parameter, allowing the visitor to select an index from the options.
    fn visit_choice(&mut self, meta: &ParamMeta, options: &'static [ChoiceOption], selected: &mut usize);
}

/// Scalar types allowed in a [`ParamGroup`].
pub trait ParamValue {
    fn accept(&mut self, meta: &ParamMeta, visitor: &mut dyn ParamVisitor);
    fn reset(&mut self, meta: &ParamMeta);
    fn is_default(&self, meta: &ParamMeta) -> bool;
}

impl ParamValue for f32 {
    fn accept(&mut self, meta: &ParamMeta, visitor: &mut dyn ParamVisitor) {
        visitor.visit_f32(meta, self);
    }
    fn reset(&mut self, meta: &ParamMeta) {
        meta.reset_f32(self);
    }
    fn is_default(&self, meta: &ParamMeta) -> bool {
        meta.is_default_f32(*self)
    }
}

impl ParamValue for usize {
    fn accept(&mut self, meta: &ParamMeta, visitor: &mut dyn ParamVisitor) {
        visitor.visit_usize(meta, self);
    }
    fn reset(&mut self, meta: &ParamMeta) {
        meta.reset_usize(self);
    }
    fn is_default(&self, meta: &ParamMeta) -> bool {
        meta.is_default_usize(*self)
    }
}

impl ParamValue for bool {
    fn accept(&mut self, meta: &ParamMeta, visitor: &mut dyn ParamVisitor) {
        visitor.visit_bool(meta, self);
    }
    fn reset(&mut self, meta: &ParamMeta) {
        meta.reset_bool(self);
    }
    fn is_default(&self, meta: &ParamMeta) -> bool {
        meta.is_default_bool(*self)
    }
}

impl<T: ParamChoice> ParamValue for T {
    fn accept(&mut self, meta: &ParamMeta, visitor: &mut dyn ParamVisitor) {
        let mut selected = self.index();
        visitor.visit_choice(meta, T::OPTIONS, &mut selected);
        if let Some(choice) = T::from_index(selected) {
            *self = choice;
        }
    }
    fn reset(&mut self, meta: &ParamMeta) {
        let default = match meta.default {
            ParamDefault::Choice(i) => T::from_index(i),
            _ => None,
        };
        if let Some(choice) = default {
            *self = choice;
        }
    }
    fn is_default(&self, meta: &ParamMeta) -> bool {
        meta.default == ParamDefault::Choice(self.index())
    }
}

/// A titled, resettable set of parameters (object-safe so groups can be listed together).
pub trait ParamGroup {
    /// Stable machine-readable identifier, e.g. "sasa", "contacts".
    fn key(&self) -> &'static str;
    /// Heading shown in parameter editors.
    fn name(&self) -> &'static str;
    /// Didactic overview explaining what this parameter group governs.
    fn description(&self) -> &'static str;
    /// Calls `visitor` once per field, in declaration order.
    fn visit(&mut self, visitor: &mut dyn ParamVisitor);
    /// Restores every field to its default.
    fn reset(&mut self);
    /// True if every field equals its default.
    fn is_default(&self) -> bool;
}

/// Reads the `#[desc = "..."]` attribute if present, otherwise concatenates all `#[doc = "..."]` attributes.
#[macro_export]
#[doc(hidden)]
macro_rules! __param_desc {
    (#[desc = $desc:literal] $(#[doc = $doc:literal])*) => { $desc };
    ($(#[doc = $doc:literal])*) => { concat!("", $($doc),*) };
}

/// Evaluates an optional unit literal to a string literal, defaulting to `""`.
#[macro_export]
#[doc(hidden)]
macro_rules! __param_unit {
    ($unit:literal) => { $unit };
    () => { "" };
}

/// Parses a `param_group!` declaration and emits the [`ParamGroup`] struct and trait implementations.
#[macro_export]
#[doc(hidden)]
macro_rules! __param_group_impl {
    // Terminal case: emit the struct and trait implementations
    (
        @accum
        header: [
            $(#[doc = $sdoc:literal])*
            $(#[desc = $sdesc:literal])?
            pub struct $name:ident($key:literal, $title:literal)
        ]
        fields: [ $( $(#[doc = $doc:literal])* pub $field:ident : $ty:ty, )* ]
        defaults: [ $( $dfield:ident : $dval:expr, )* ]
        visits: [ $( [ $vfield:ident, $vname:expr, $vunit:expr, $vdesc:expr, $vmin:expr, $vmax:expr, $vdefault:expr ] )* ]
        body: []
    ) => {
        $(#[doc = $sdoc])*
        #[derive(::serde::Serialize, ::serde::Deserialize, Clone, Debug, PartialEq)]
        #[serde(default)]
        pub struct $name {
            $( $(#[doc = $doc])* pub $field: $ty, )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $( $dfield: $dval, )* }
            }
        }

        impl $crate::params::ParamGroup for $name {
            fn key(&self) -> &'static str {
                $key
            }

            fn name(&self) -> &'static str {
                $title
            }

            fn description(&self) -> &'static str {
                $crate::__param_desc!($(#[desc = $sdesc])? $(#[doc = $sdoc])*)
            }

            fn visit(&mut self, visitor: &mut dyn $crate::params::ParamVisitor) {
                $(
                    let meta = $crate::params::ParamMeta {
                        key: stringify!($vfield),
                        name: $vname,
                        unit: $vunit,
                        description: $vdesc,
                        min: $vmin,
                        max: $vmax,
                        default: $crate::params::IntoParamDefault::into_param_default($vdefault),
                    };
                    $crate::params::ParamValue::accept(&mut self.$vfield, &meta, visitor);
                )*
            }

            fn reset(&mut self) {
                *self = Self::default();
            }

            fn is_default(&self) -> bool {
                *self == Self::default()
            }
        }
    };

    // Ranged field (numeric with range, optional unit)
    (
        @accum
        header: [ $($h:tt)* ]
        fields: [ $($f:tt)* ]
        defaults: [ $($d:tt)* ]
        visits: [ $($v:tt)* ]
        body: [
            $(#[doc = $doc:literal])*
            $(#[desc = $desc:literal])?
            $field:ident : $ty:ty = $default:expr, $min:literal ..= $max:literal, $name_lit:literal $(, $unit_lit:literal)? ;
            $( $rest:tt )*
        ]
    ) => {
        $crate::__param_group_impl! {
            @accum
            header: [ $($h)* ]
            fields: [ $($f)* $(#[doc = $doc])* pub $field: $ty, ]
            defaults: [ $($d)* $field: $default, ]
            visits: [
                $($v)*
                [
                    $field,
                    $name_lit,
                    $crate::__param_unit!($($unit_lit)?),
                    $crate::__param_desc!($(#[desc = $desc])? $(#[doc = $doc])*),
                    Some($min as f64),
                    Some($max as f64),
                    $default
                ]
            ]
            body: [ $( $rest )* ]
        }
    };

    // Un-ranged field (bools, or any scalar without a range, optional unit)
    (
        @accum
        header: [ $($h:tt)* ]
        fields: [ $($f:tt)* ]
        defaults: [ $($d:tt)* ]
        visits: [ $($v:tt)* ]
        body: [
            $(#[doc = $doc:literal])*
            $(#[desc = $desc:literal])?
            $field:ident : $ty:ty = $default:expr, $name_lit:literal $(, $unit_lit:literal)? ;
            $( $rest:tt )*
        ]
    ) => {
        $crate::__param_group_impl! {
            @accum
            header: [ $($h)* ]
            fields: [ $($f)* $(#[doc = $doc])* pub $field: $ty, ]
            defaults: [ $($d)* $field: $default, ]
            visits: [
                $($v)*
                [
                    $field,
                    $name_lit,
                    $crate::__param_unit!($($unit_lit)?),
                    $crate::__param_desc!($(#[desc = $desc])? $(#[doc = $doc])*),
                    None,
                    None,
                    $default
                ]
            ]
            body: [ $( $rest )* ]
        }
    };
}

/// Declares a parameter group with `Default`, `Serialize`, `Deserialize`, and `ParamGroup`.
#[macro_export]
macro_rules! param_group {
    (
        $(#[doc = $sdoc:literal])*
        $(#[desc = $sdesc:literal])?
        pub struct $name:ident($key:literal, $title:literal) {
            $( $body:tt )*
        }
    ) => {
        $crate::__param_group_impl! {
            @accum
            header: [
                $(#[doc = $sdoc])*
                $(#[desc = $sdesc])?
                pub struct $name($key, $title)
            ]
            fields: []
            defaults: []
            visits: []
            body: [ $( $body )* ]
        }
    };
}

/// Declares an enum usable as a parameter: derives `Serialize`/`Deserialize`
/// (each variant serialized as its key), `Default` (the first variant), and [`ParamChoice`].
///
/// # Example
/// ```ignore
/// param_choice! {
///     /// How to shape the interface mesh.
///     pub enum ShapeBy {
///         #[desc = "Bisector between the partners' vdW surfaces."]
///         AtomSurfaces("atom_surfaces", "Atom surfaces"),
///         /// Doc comments work as descriptions too, as in `param_group!`.
///         InteractionMidpoints("interaction_midpoints", "Interaction midpoints"),
///     }
/// }
/// ```
#[macro_export]
macro_rules! param_choice {
    (
        $(#[doc = $edoc:literal])*
        pub enum $name:ident {
            $(
                $(#[doc = $doc:literal])*
                $(#[desc = $desc:literal])?
                $variant:ident($key:literal, $title:literal)
            ),+ $(,)?
        }
    ) => {
        $(#[doc = $edoc])*
        #[derive(::serde::Serialize, ::serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $( $(#[doc = $doc])* #[serde(rename = $key)] $variant, )+
        }

        impl $name {
            const VARIANTS: &'static [Self] = &[ $( Self::$variant ),+ ];
        }

        impl Default for $name {
            fn default() -> Self {
                Self::VARIANTS[0]
            }
        }

        impl $crate::params::ParamChoice for $name {
            const OPTIONS: &'static [$crate::params::ChoiceOption] = &[
                $(
                    $crate::params::ChoiceOption {
                        key: $key,
                        name: $title,
                        description: $crate::__param_desc!($(#[desc = $desc])? $(#[doc = $doc])*),
                    },
                )+
            ];

            fn index(self) -> usize {
                Self::VARIANTS.iter().position(|v| *v == self).expect("every variant is listed")
            }

            fn from_index(index: usize) -> Option<Self> {
                Self::VARIANTS.get(index).copied()
            }
        }
    };
}

/// Every tunable used by `build_interface_map`, surface meshing, and interface geometry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AnalysisParams {
    pub sasa: SasaParams,
    pub contacts: ContactParams,
    pub relax: RelaxParams,
    pub contour: ContourParams,
    pub surface: SurfaceParams,
    pub interface_mesh: InterfaceMeshParams,
    pub geometry: GeometryParams,
    pub cavities: CavityParams,
}

impl AnalysisParams {
    /// All groups in pipeline order, for generic editors.
    pub fn groups_mut(&mut self) -> [&mut dyn ParamGroup; 8] {
        [
            &mut self.sasa,
            &mut self.contacts,
            &mut self.relax,
            &mut self.contour,
            &mut self.surface,
            &mut self.interface_mesh,
            &mut self.geometry,
            &mut self.cavities,
        ]
    }
}
