//! Le calcul des modèles, au plus près du matériel de chaque système.

#[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
pub mod mlx_smoke;
