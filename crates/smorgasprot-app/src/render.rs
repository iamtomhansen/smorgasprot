//! GPU rendering backends (`wgpu`) for 3D molecular geometry, decoupled from `viewport` scenes.

pub mod surface;

pub use surface::{GpuDiscDraw, GpuLineDraw, GpuMeshDraw, GpuMeshKey, GpuPolyTri, GpuSurfaceRenderer};
