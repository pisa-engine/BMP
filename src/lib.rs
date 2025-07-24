#![cfg_attr(all(target_arch = "aarch64"), feature(stdarch_aarch64_prefetch))]
#![recursion_limit = "1024"]

pub mod ciff;
pub mod index;
mod proto;
pub mod query;
pub mod search;
pub mod util;
pub mod benchmark; // New benchmarking module

pub use ciff::CiffToBmp;

// Performance configuration for adaptive optimization
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    pub use_simd: bool,
    pub use_blas: bool,
    pub use_gpu: bool,
    pub use_sparse: bool,
    pub prefetch_distance: usize,
    pub parallel_threshold: usize,
    pub gpu_batch_size: usize,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        PerformanceConfig {
            use_simd: true,
            use_blas: true,
            use_gpu: cfg!(feature = "gpu"),
            use_sparse: true,
            prefetch_distance: 4,
            parallel_threshold: 1000,
            gpu_batch_size: 32,
        }
    }
}

impl PerformanceConfig {
    pub fn for_dataset_size(num_docs: usize, num_queries: usize) -> Self {
        let mut config = Self::default();
        
        // Adaptive configuration based on dataset characteristics
        if num_docs > 1_000_000 && num_queries > 1000 {
            config.use_gpu = true;
            config.gpu_batch_size = 64;
            config.prefetch_distance = 8;
        } else if num_docs > 100_000 {
            config.use_blas = true;
            config.parallel_threshold = 500;
            config.prefetch_distance = 6;
        } else {
            config.use_gpu = false;
            config.prefetch_distance = 2;
        }
        
        config
    }
}
