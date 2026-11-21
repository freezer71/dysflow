//! Un calcul MLX minimal : prouve que MLX, compilé par `mlx-rs`, tourne dans
//! le processus (le terminal, ou l'app à côté de mlx-swift).

use mlx_rs::{Array, Device};

/// La somme de `[1, 2, 3]` calculée sur le GPU par défaut : 6.
pub fn smoke() -> anyhow::Result<f32> {
    mlx_rs::with_device(Device::gpu(), || {
        let sum = Array::from_slice(&[1.0f32, 2.0, 3.0], &[3]).sum(None)?;
        sum.eval()?;
        Ok(sum.try_item_exact::<f32>()?)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn mlx_sums_on_the_gpu() {
        assert_eq!(super::smoke().expect("MLX"), 6.0);
    }
}
