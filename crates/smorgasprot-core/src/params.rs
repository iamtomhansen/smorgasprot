//! Tunable analysis parameters: traits, metadata, visitor pattern, and pipeline aggregation.
//! Subsystems declare their own parameter groups via [`param_group!`]; this module re-exports them.

use serde::{Deserialize, Serialize};

pub use crate::contour::ContourParams;
pub use crate::flatten::RelaxParams;
pub use crate::interface_geometry::GeometryParams;
pub use crate::physics::ContactParams;
pub use crate::sasa::SasaParams;
pub use crate::surface::SurfaceParams;

/// Default value of a parameter, matching its scalar type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamDefault {
    F32(f32),
    Usize(usize),
    Bool(bool),
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

/// Every tunable used by `build_interface_map`, surface meshing, and interface geometry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AnalysisParams {
    pub sasa: SasaParams,
    pub contacts: ContactParams,
    pub relax: RelaxParams,
    pub contour: ContourParams,
    pub surface: SurfaceParams,
    pub geometry: GeometryParams,
}

impl AnalysisParams {
    /// All groups in pipeline order, for generic editors.
    pub fn groups_mut(&mut self) -> [&mut dyn ParamGroup; 6] {
        [&mut self.sasa, &mut self.contacts, &mut self.relax, &mut self.contour, &mut self.surface, &mut self.geometry]
    }
}
